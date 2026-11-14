# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs cohesive scalability harness (not a port of Akantu code): builders
# of synthetic run directories for the harness unit tests. The physical
# outputs are taken from the committed fixtures/perf_cohesive/ (so the
# consistency checks run on real driver output); run.json, build records,
# Chrono rows and logs are synthesised in the format the harness writes.

import hashlib
import json
import os
import shutil
import sys

SCRIPTS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, SCRIPTS)
import make_fixtures as mf  # noqa: E402

COMMITTED = os.path.join(mf.REPO, "fixtures", "perf_cohesive")
RETRY_WARNING = ("!!! WARNING: MUMPS dynamic memory is insufficient it will be increased "
                 "allowed to use 10% more\n")


def sha(data):
    return hashlib.sha256(data).hexdigest()


def chrono_csv(ranks, steps, cohesive, energy=False):
    phases = {"before_step": 1, "dist_mesh": 1, "init_full": 1, "initial_conditons": 1,
              "read_mesh": 1, "solve_step": steps, "static_solve": 1}
    if steps >= 100:
        phases["boundary_conditions"] = steps // 100
    if cohesive:
        phases["check_cohesive_stress"] = steps
    if energy:
        phases["energies"] = steps
    names = sorted(phases)
    header = '"psize"' + "".join(f', "{n}", "{n} nb_rep"' for n in names)
    row = str(ranks) + "".join(f", {0.001 * (i + 1)}, {phases[n]}" for i, n in enumerate(names))
    return (header + "\n" + row + "\n").encode()


def build_record(config, image_id="sha256:test-image", tag=""):
    return {"format": "cohesive-perf build record v1", "config": config,
            "binary": {"path": f"/perf/builds/{config}/cohesive_extrinsic",
                       "sha256": sha(f"binary-{config}{tag}".encode()), "size": 1},
            "image_id": image_id, "harness_sha256": {}, "libakantu": {"sha256": "ab"}}


def run_json(config, ranks, material, mesh, mesh_sha, steps, extra, fnte, rec, input_data,
             started="2026-11-11T12:00:00Z", **over):
    digest = mf.build_digest(rec)
    d = {"format": "cohesive-perf run v2", "config": config, "ranks": ranks,
         "material": material, "mesh": mesh, "mesh_sha256": mesh_sha, "max_steps": steps,
         "extra_parameters": list(extra), "input_sha256": sha(input_data),
         "binary_sha256": rec["binary"]["sha256"], "started_utc": started, "exit_status": 0,
         "wall_s": 12.5, "omp_num_threads": "1", "aka_seed": "1", "scotch_pthread_number": "1",
         "fill_nodes_to_elements": fnte, "container_cpus": 8, "oversubscribed": False,
         "host": {"machine": "x86_64", "cpu_model": "Test CPU", "kernel": "6", "logical_cpus": 8},
         "image": "akantu-oracle:dev", "image_id": rec["image_id"],
         "build_record_sha256": digest, "runtime_check": {"build_record_sha256": digest},
         "harness_matches_build": True}
    d.update(over)
    return json.dumps(d, indent=1).encode()


def denormalise_summary(data, mesh):
    s = json.loads(data)
    s["mesh"] = mesh
    s["loop_time_s"] = 1.0
    s["init_time_s"] = 0.5
    return (json.dumps(s, indent=1) + "\n").encode()


def denormalise_rank(data):
    s = json.loads(data)
    s["fill_nodes_to_elements"]["event_time_s"] = 0.25
    s["fill_nodes_to_elements"]["other_time_s"] = 0.0
    return (json.dumps(s) + "\n").encode()


def fixture_run(case, config, records):
    """Files of one gen-fixtures.sh run, from the committed case data."""
    spec = mf.SPECS[case]
    src_case = {"coh_np2_distribute": "coh_np2", "coh_np1_fnte_joss": "coh_np1"}.get(case, case)
    src = os.path.join(COMMITTED, src_case)
    files = {}
    for name in os.listdir(src):
        data = mf.perfcheck.read_bytes(os.path.join(src, name))
        if name == "summary.json":
            data = denormalise_summary(data, mf.MESH_IN_VOLUME)
        elif name.startswith("summary_rank"):
            data = denormalise_rank(data)
        elif name == "input.dat":
            continue
        files[name] = data
    if case == "coh_np2_distribute":
        files.pop("partition.csv")
    if case == "coh_np1_fnte_joss":
        s = json.loads(files["summary.json"])
        s["fill_nodes_to_elements"] = "joss"
        files["summary.json"] = (json.dumps(s, indent=1) + "\n").encode()
        r = json.loads(files["summary_rank0.json"])
        f = r["fill_nodes_to_elements"]
        f.update(mode="joss", event_calls_skipped=f["event_calls"], event_calls=0)
        files["summary_rank0.json"] = (json.dumps(r) + "\n").encode()
    extra = mf.COMMON + spec["extra"]
    steps = 1500
    files["input.dat"] = mf.expected_input(spec["material"], steps, mf.MESH_IN_VOLUME, extra)
    files["chrono.csv"] = chrono_csv(spec["ranks"], steps, spec["material"] == "cohesive", True)
    files["err.txt"] = files["chrono.csv"]
    files["out.txt"] = b"Time step: 1.6e-07\n"
    rec = records[config]
    files["build-record.json"] = (json.dumps(rec, indent=1, sort_keys=True) + "\n").encode()
    mesh_sha = sha(mf.perfcheck.read_bytes(mf.FROZEN_MESH))
    files["run.json"] = run_json(config, spec["ranks"], spec["material"], mf.MESH_IN_VOLUME,
                                 mesh_sha, steps, extra, spec["fnte"], rec, files["input.dat"])
    return files


def perturb_energies(data, row, digit_delta=1):
    """Change the last digit of the ep value of one row (a rounding-level change)."""
    lines = data.decode().split("\n")
    fields = lines[row + 1].split(", ")
    v = fields[2]
    last = int(v[-1])
    fields[2] = v[:-1] + str((last + digit_delta) % 10)
    lines[row + 1] = ", ".join(fields)
    return "\n".join(lines).encode()


def fixture_runs(reps=3, nonrepeatable_np2=True):
    """{run key: files} of a complete gen-fixtures.sh campaign."""
    records = {"strict": build_record("strict"), "default": build_record("default")}
    runs = {}
    for config, case, rep in mf.plan(reps):
        files = fixture_run(case, config, records)
        if nonrepeatable_np2 and case in ("coh_np2", "coh_np2_global") and rep == 3:
            files["energies.csv"] = perturb_energies(files["energies.csv"], 700)
        runs[f"fixtures/{config}/{case}/rep{rep}"] = files
    return runs


def write_runs(runs, root):
    for key, files in runs.items():
        d = os.path.join(root, key)
        os.makedirs(d, exist_ok=True)
        for name, data in files.items():
            with open(os.path.join(d, name), "wb") as f:
                f.write(data)


def sandbox_fixtures(tmp):
    """A temporary fixtures/ tree holding a copy of perf_cohesive/ and the
    SHA256SUMS lines of it; returns its path."""
    fx = os.path.join(tmp, "fixtures")
    shutil.copytree(COMMITTED, os.path.join(fx, "perf_cohesive"))
    lines = [line for line in mf.perfcheck.read_text(os.path.join(mf.REPO, "fixtures", "SHA256SUMS"))
             .splitlines(keepends=True) if line.split("  ", 1)[1].startswith("perf_cohesive/")]
    with open(os.path.join(fx, "SHA256SUMS"), "w") as f:
        f.write("0" * 64 + "  aaa/other.npy\n")
        f.writelines(lines)
    return fx
