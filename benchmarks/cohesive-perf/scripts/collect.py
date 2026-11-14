#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs cohesive scalability harness (not a port of Akantu code).
"""Collect the run directories of a campaign into results/<campaign>/.

    python3 -I scripts/collect.py [--allow-incomplete] [--allow-legacy-provenance]
                                  [--runs-dir DIR] <campaign>

Reads /perf/runs/<campaign>/ from the scratch volume (through `docker run`,
the campaign passed as an argument, never interpolated into shell code), or
DIR/<campaign>/ for a local copy, and writes results/<campaign>/:

  timmings_<material>_<variant>_h<h>.csv  rank-0 Chrono CSV rows in the
                                          upstream format (one row per valid run)
  runs.jsonl        one record per run directory: run.json (provenance) +
                    summary.json + per-rank summaries + the parsed Chrono row
                    + its status (valid / invalid with reasons / invalid
                    attempt / oversubscribed)
  provenance.json   collecting host, image, and the build records carried by
                    the runs themselves (never the current build directory)
  collection.json   completeness report: failures, warnings, rejected
                    attempts and their cost, runs to repeat

Checks:
* the whole results/<campaign>/ directory is the output: it is written to a
  staging directory and swapped in only at the end, so obsolete tables and
  runs never survive a new collection;
* a run is valid only if its exit status is 0, no MUMPS error/warning is in
  its output (UD-216 retry: exit 0 but a wrong static solution), its Chrono
  row is complete and consistent with ranks and steps, its summaries are
  consistent, and it carries a build record matching its run-time check;
* runs combined in one table must share one measurement identity (mesh
  digest, steps, seed, Scotch threads, interposer mode, extra parameters,
  build record, image, host CPU); a table mixing identities is not written;
* every run of the campaign's plans (run-series.sh) must be present, valid,
  newer than its plan and match it; a run directory without plan must be
  valid. Anything else is a failure: exit status 1 (the tables of the valid
  runs are still written, collection.json says `complete: false`), unless
  --allow-incomplete (used by rerun-invalid.sh to list what to repeat);
* oversubscribed runs (more ranks than container CPUs) are valid
  physics but excluded from the timing tables.
"""

import argparse
import hashlib
import io
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import tarfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import perfcheck  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
PERF_DIR = os.path.dirname(HERE)
RESULTS = os.path.join(PERF_DIR, "results")
RESERVED = ("published-jed",)  # verbatim upstream CSVs, never a campaign output
VOLUME = os.environ.get("PERF_VOLUME", "akantu-perf-cohesive")
IMAGE = os.environ.get("PERF_IMAGE", "akantu-oracle:dev")
WANTED = ("run.json", "summary.json", "err.txt", "out.txt", "chrono.csv", "insertions.csv",
          "energies.csv", "time.txt", "build-record.json", "refused.txt")
ATTEMPT_RE = re.compile(r"/rep\d+\.invalid-\d+$")
REP_RE = re.compile(r"/rep\d+$")


def fetch_volume(campaign):
    """Tar the small files of every run directory out of the volume."""
    names = " -o ".join("-name '%s'" % w for w in WANTED)
    script = ('cd /perf/runs && test -d "$1" && find "$1" -type f \\( %s '
              "-o -name 'summary_rank*.json' -o -path '*/_plans/*.json' \\) "
              "-print0 | tar --null -T - -cf -") % names
    tar = subprocess.run(["docker", "run", "--rm", "-v", f"{VOLUME}:/perf:ro", IMAGE,
                          "bash", "-c", script, "_", campaign],
                         check=True, capture_output=True).stdout
    files = {}
    with tarfile.open(fileobj=io.BytesIO(tar)) as t:
        for m in t.getmembers():
            if m.isfile():
                files[m.name] = t.extractfile(m).read()
    return files


def fetch_dir(root, campaign):
    files = {}
    base = os.path.join(root, campaign)
    for d, _, names in os.walk(base):
        for n in names:
            p = os.path.join(d, n)
            files[os.path.relpath(p, root)] = perfcheck.read_bytes(p)
    return files


def machine():
    def sh(*cmd):
        try:
            return subprocess.run(cmd, capture_output=True, text=True, check=True).stdout.strip()
        except (OSError, subprocess.CalledProcessError):
            return None
    return {"collecting_host_os": platform.platform(),
            "collecting_host_machine": platform.machine(),
            "docker_cpus": sh("docker", "info", "--format", "{{.NCPU}}"),
            "docker_mem_bytes": sh("docker", "info", "--format", "{{.MemTotal}}"),
            "docker_kernel": sh("docker", "info", "--format", "{{.KernelVersion}}"),
            "docker_server": sh("docker", "info", "--format",
                                "{{.ServerVersion}} {{.OperatingSystem}}"),
            "harness_git_commit": sh("git", "-C", PERF_DIR, "rev-parse", "HEAD"),
            "harness_git_dirty": bool(sh("git", "-C", PERF_DIR, "status", "--porcelain", "."))}


def build_digest(rec):
    return hashlib.sha256(json.dumps(rec, sort_keys=True, separators=(",", ":"))
                          .encode()).hexdigest()


def text(r, name):
    return r.get(name, b"").decode("utf-8", "replace")


def variant_of(rec):
    variant = rec["config"]
    if rec.get("fill_nodes_to_elements", "upstream") != "upstream":
        variant += "-fnte_" + rec["fill_nodes_to_elements"]
    if "blocked_node=global" in rec.get("extra_parameters", []):
        variant += "-global"
    return variant


def mesh_label(rec):
    base = os.path.basename(rec["mesh"])
    return base[len("cube-"):-len(".msh")] if base.startswith("cube-") else base[:-len(".msh")]


def identity(rec):
    """What runs combined in one table must share (ranks and rep excluded)."""
    host = rec.get("host") or {}
    return {"mesh_sha256": rec.get("mesh_sha256"), "max_steps": rec.get("max_steps"),
            "aka_seed": rec.get("aka_seed"), "scotch_pthread_number": rec.get("scotch_pthread_number"),
            "omp_num_threads": rec.get("omp_num_threads"),
            "fill_nodes_to_elements": rec.get("fill_nodes_to_elements"),
            "extra_parameters": sorted(rec.get("extra_parameters", [])),
            "build": rec.get("build_record_sha256") or ("legacy binary " + str(rec.get("binary_sha256"))),
            "image_id": rec.get("image_id"), "machine": host.get("machine"),
            "cpu_model": host.get("cpu_model")}


PLAN_KEYS = ("config", "material", "ranks", "mesh", "max_steps", "aka_seed",
             "fill_nodes_to_elements", "extra_parameters")


def analyse_run(d, r, allow_legacy):
    """Run record with status and problems."""
    p = []
    rec = perfcheck.load_json(r["run.json"], "run.json", p) or {}
    rec["run_dir"] = d
    rec["invalid_attempt"] = bool(ATTEMPT_RE.search(d))
    if rec.get("exit_status") != 0:
        p.append(f"exit status {rec.get('exit_status')}")
    for name in ("err.txt", "out.txt"):
        if name not in r:
            p.append(f"{name} missing")
    rec["mumps_messages"] = perfcheck.mumps_findings(text(r, "err.txt"), text(r, "out.txt"))
    rec["mumps_memory_retries"] = rec["mumps_messages"].get(perfcheck.MUMPS_MARKERS[0], 0)
    for marker, n in rec["mumps_messages"].items():
        p.append(f"MUMPS message {marker!r} x{n}")
    ranks, steps = rec.get("ranks"), rec.get("max_steps")
    if not isinstance(ranks, int) or not isinstance(steps, int):
        p.append("run.json ranks/max_steps missing")
        ranks, steps = 0, 0
    summary = (perfcheck.load_json(r["summary.json"], "summary.json", p)
               if "summary.json" in r else None)
    rec["summary"] = summary or {}
    per_rank = {}
    for name in sorted(r):
        m = re.match(r"^summary_rank(\d+)\.json$", name)
        if m:
            v = perfcheck.load_json(r[name], name, p)
            if v is not None:
                per_rank[int(m.group(1))] = v
    rec["per_rank"] = [per_rank[k] for k in sorted(per_rank)]
    # chrono.csv (written by the driver since the stderr loss was seen) is
    # preferred; stderr is the upstream channel.
    hdr, row, parsed = perfcheck.parse_chrono(text(r, "chrono.csv"))
    rec["chrono_source"] = "chrono.csv"
    if parsed is None:
        hdr, row, parsed = perfcheck.parse_chrono(text(r, "err.txt"))
        rec["chrono_source"] = "stderr" if parsed else "malformed"
    rec["chrono"] = parsed
    material = rec.get("material")
    p += perfcheck.check_chrono(parsed, ranks, steps, material == "cohesive")
    if summary is None:
        p.append("summary.json missing or invalid")
    else:
        energies, ins = [], None
        if "energies.csv" in r:
            energies, ep = perfcheck.parse_energies(r["energies.csv"], steps)
            p += ep
        if "insertions.csv" in r:
            ins, ip = perfcheck.parse_insertions(r["insertions.csv"], steps)
            p += ip
        else:
            p.append("insertions.csv missing")
        expect = {"cohesive_insertion": material == "cohesive",
                  "blocked_node": "global" if "blocked_node=global" in rec.get(
                      "extra_parameters", []) else "local",
                  "fill_nodes_to_elements": rec.get("fill_nodes_to_elements"),
                  "mesh": rec.get("mesh")}
        p += perfcheck.check_summary(summary, ranks, steps, energies, ins, per_rank, expect)
    rec["insertions_csv_lines"] = max(0, len(text(r, "insertions.csv").splitlines()) - 1)
    # build record carried by the run
    if "build-record.json" in r and rec.get("build_record_sha256"):
        b = perfcheck.load_json(r["build-record.json"], "build-record.json", p)
        if b is not None:
            dg = build_digest(b)
            if dg != rec["build_record_sha256"] or \
                    (rec.get("runtime_check") or {}).get("build_record_sha256") != dg:
                p.append("build record does not match the run's record digest/run-time check")
            if (b.get("binary") or {}).get("sha256") != rec.get("binary_sha256"):
                p.append("binary digest differs from the build record")
            rec["_build_record"] = b
    elif allow_legacy:
        rec["legacy_provenance"] = True
    else:
        p.append("no build record (legacy run: re-run it, or --allow-legacy-provenance)")
    rec["problems"] = p
    if rec["invalid_attempt"]:
        rec["status"] = "invalid attempt"
    elif p:
        rec["status"] = "invalid"
    elif rec.get("oversubscribed"):
        rec["status"] = "oversubscribed"
    else:
        rec["status"] = "valid"
    rec["valid"] = rec["status"] in ("valid", "oversubscribed")
    rec["variant"] = variant_of(rec) if "config" in rec else None
    return rec, hdr, row


def collect(campaign, files, allow_legacy):
    """(records, tables, report) of a campaign's files {relative path: bytes}."""
    runs, plans = {}, []
    for name, data in files.items():
        d, base = os.path.split(name)
        if d == f"{campaign}/_plans":
            plans.append((name, data))
        else:
            runs.setdefault(d, {})[base] = data
    failures, warnings = [], []
    expected = {}
    for name, data in sorted(plans):
        try:
            plan = json.loads(data)
            for e in plan["expected_runs"]:
                expected[e["run_dir"]] = dict(e, _plan=name, _created=plan["created_utc"])
        except (ValueError, KeyError, TypeError):
            failures.append(f"unreadable plan {name}")
    records, tables, meta_rows = [], {}, {}
    for d in sorted(runs):
        r = runs[d]
        if "run.json" not in r:
            if "refused.txt" in r:
                failures.append(f"{d}: run refused: {text(r, 'refused.txt').strip()}")
            continue
        rec, hdr, row = analyse_run(d, r, allow_legacy)
        records.append(rec)
        if rec["status"] == "valid" and row is not None:
            key = (rec["material"], rec["variant"], mesh_label(rec))
            t = tables.setdefault(key, {"header": hdr, "rows": [], "identities": []})
            if t["header"] != hdr:
                t["mixed_header"] = True
            t["rows"].append((rec["ranks"], d, row))
            t["identities"].append(identity(rec))
        if rec["status"] == "oversubscribed":
            warnings.append(f"{d}: oversubscribed ({rec['ranks']} ranks on "
                            f"{rec.get('container_cpus')} CPUs): excluded from timing tables")
        if rec.get("harness_matches_build") is False:
            warnings.append(f"{d}: harness sources changed between build and run "
                            "(the executable is the recorded one)")
        meta_rows[d] = rec
    # expected runs (plans) and unplanned run directories
    rerun = []
    for d, e in sorted(expected.items()):
        rec = meta_rows.get(d)
        plan_view = {k: e[k] for k in PLAN_KEYS}
        if rec is None:
            failures.append(f"{d}: planned run missing")
            rerun.append(plan_view | {"run_dir": d})
            continue
        mismatch = [k for k in PLAN_KEYS if rec.get(k) != e[k]]
        if mismatch:
            failures.append(f"{d}: run does not match its plan ({', '.join(mismatch)})")
        elif (rec.get("started_utc") or "") < e["_created"]:
            failures.append(f"{d}: run older than its plan {e['_plan']} (stale)")
        elif not rec["valid"]:
            failures.append(f"{d}: invalid: " + "; ".join(rec["problems"]))
        else:
            continue
        rerun.append(plan_view | {"run_dir": d})
    for d, rec in sorted(meta_rows.items()):
        if d in expected or rec["invalid_attempt"] or rec["valid"]:
            continue
        failures.append(f"{d}: invalid (no replacement): " + "; ".join(rec["problems"]))
        if REP_RE.search(d):
            rerun.append({k: rec.get(k) for k in PLAN_KEYS} | {"run_dir": d})
    # table identity
    written = {}
    for key, t in sorted(tables.items()):
        ids = {json.dumps(i, sort_keys=True) for i in t["identities"]}
        if len(ids) > 1 or t.get("mixed_header"):
            failures.append(f"table {key}: runs with incompatible measurement identities "
                            f"({len(ids)} distinct): not written")
            continue
        written[key] = t
    attempts = [r for r in records if r["invalid_attempt"] or r["status"] == "invalid"]
    report = {"campaign": campaign, "complete": not failures, "failures": failures,
              "warnings": warnings,
              "counts": {s: sum(1 for r in records if r["status"] == s)
                         for s in ("valid", "invalid", "invalid attempt", "oversubscribed")},
              "rejected_attempts": len(attempts),
              "rejected_attempts_wall_s": round(sum(r.get("wall_s") or 0 for r in attempts), 3),
              "plans": sorted({e["_plan"] for e in expected.values()}),
              "tables": [f"timmings_{m}_{v}_{h}.csv" for (m, v, h) in sorted(written)],
              "rerun": rerun}
    return records, written, report


def write_results(campaign, records, tables, report, prov):
    out = os.path.join(RESULTS, campaign)
    if not perfcheck.confined(RESULTS, out) or os.path.dirname(os.path.realpath(out)) != \
            os.path.realpath(RESULTS):
        raise SystemExit(f"refusing to write outside {RESULTS}: {out}")
    staging = os.path.join(RESULTS, f".{campaign}.staging-{os.getpid()}")
    shutil.rmtree(staging, ignore_errors=True)
    os.makedirs(staging)
    try:
        for (material, variant, mesh), t in sorted(tables.items()):
            with open(os.path.join(staging, f"timmings_{material}_{variant}_{mesh}.csv"), "w") as f:
                f.write(t["header"] + "\n")
                for _, _, row in sorted(t["rows"], key=lambda x: (x[0], x[1])):
                    f.write(row + "\n")
        with open(os.path.join(staging, "runs.jsonl"), "w") as f:
            for rec in records:
                f.write(json.dumps({k: v for k, v in rec.items() if not k.startswith("_")},
                                   sort_keys=True) + "\n")
        for name, obj in (("provenance.json", prov), ("collection.json", report)):
            with open(os.path.join(staging, name), "w") as f:
                json.dump(obj, f, indent=1, sort_keys=True)
                f.write("\n")
    except BaseException:
        shutil.rmtree(staging, ignore_errors=True)
        raise
    old = os.path.join(RESULTS, f".{campaign}.old-{os.getpid()}") if os.path.exists(out) else None
    if old:
        os.rename(out, old)
    try:
        os.rename(staging, out)
    except BaseException:
        if old:
            os.rename(old, out)
        shutil.rmtree(staging, ignore_errors=True)
        raise
    if old:
        shutil.rmtree(old)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("campaign")
    ap.add_argument("--allow-incomplete", action="store_true")
    ap.add_argument("--allow-legacy-provenance", action="store_true")
    ap.add_argument("--runs-dir")
    a = ap.parse_args()
    try:
        campaign = perfcheck.check_campaign(a.campaign)
    except ValueError as e:
        sys.exit(str(e))
    if campaign in RESERVED:
        sys.exit(f"{campaign} is reserved (verbatim upstream data)")
    files = fetch_dir(a.runs_dir, campaign) if a.runs_dir else fetch_volume(campaign)
    if not files:
        sys.exit(f"no run directory found for campaign {campaign}")
    records, tables, report = collect(campaign, files, a.allow_legacy_provenance)
    builds = {}
    for rec in records:
        if "_build_record" in rec:
            builds[rec["build_record_sha256"]] = rec["_build_record"]
    prov = machine() if not a.runs_dir else {}
    prov.update({"campaign": campaign, "build_records": builds,
                 "legacy_runs_without_build_record": sum(1 for r in records
                                                        if r.get("legacy_provenance")),
                 "image_ids": sorted({r.get("image_id") for r in records if r.get("image_id")}),
                 "run_hosts": sorted({json.dumps(r.get("host"), sort_keys=True)
                                      for r in records if r.get("host")})})
    out = write_results(campaign, records, tables, report, prov)
    print(f"{len(records)} runs, {len(tables)} tables -> {os.path.relpath(out)}")
    for w in report["warnings"]:
        print("warning:", w, file=sys.stderr)
    if report["failures"]:
        for f in report["failures"]:
            print("FAILURE:", f, file=sys.stderr)
        print(f"{len(report['failures'])} failure(s): campaign incomplete "
              "(scripts/rerun-invalid.sh repeats invalid and missing runs)", file=sys.stderr)
        if not a.allow_incomplete:
            sys.exit(1)


if __name__ == "__main__":
    main()
