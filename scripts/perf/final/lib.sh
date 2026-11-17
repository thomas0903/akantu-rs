# shellcheck shell=bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): shared helpers of the
# final measurement campaign (scripts/perf/final/README.md). Sourced by
# prepare.sh and run_part.sh; defines no behaviour on its own.
#
# Layout (FINAL_ROOT, default ~/final):
#   FINAL_ROOT/bin/<variant>/   binaries of one port variant (prepare.sh):
#       explicit_step_timing, suite_solid, suite_models, suite_setup (host),
#       mpi/cohesive_extrinsic, mpi/suite_solid_mumps (mpi,scotch,mumps),
#       mpi/suite_{solid,models,setup} (mpi,scotch), mpi/mpi_bench,
#       venv/ (Python with the akantu extension), VARIANT.json
#   FINAL_ROOT/bin/cpp/         C++ explicit-step harness binaries, CPP.json
#   FINAL_ROOT/bin/raw/         bench_baseline (raw MPI, C)
#   FINAL_ROOT/data/<part>/     one data directory per part (run_part.sh)
# The port's MPI drivers are also copied into the Docker volume
# akantu-rs-mpi-target at /target/final/<variant>/ (the campaign tool
# c1prep_campaign.sh runs them from there).

FIN_HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIN_REPO="$(cd "${FIN_HERE}/../../.." && pwd)"
FINAL_ROOT="${FINAL_ROOT:-${HOME}/final}"
# shellcheck disable=SC2034 # read by the scripts that source this file
FIN_BASELINE="${FINAL_BASELINE:-${HOME}/baseline}"
# shellcheck disable=SC2034 # likewise
FIN_TVOL="${FINAL_TARGET_VOLUME:-akantu-rs-mpi-target}"
FIN_MPI_IMAGE="${FINAL_MPI_IMAGE:-akantu-rs-mumps:dev}"
FIN_ORACLE_IMAGE="${FINAL_ORACLE_IMAGE:-akantu-oracle:dev}"
# the C++ reference commit (docs/REFERENCE.md)
FIN_CPP_REF=35a851780d0b9471d258ed53a003ae98aaa95ea0

# rustup's environment when the shell did not load it (ssh without a login shell)
if ! command -v cargo > /dev/null 2>&1 && [ -f "${HOME}/.cargo/env" ]; then
  # shellcheck source=/dev/null
  . "${HOME}/.cargo/env"
fi

fin_utc() { date -u +%FT%TZ; }
fin_die() { echo "$(fin_utc) FATAL: $*" >&2; exit 2; }

# Rewrite a text so that it names no machine: the data root becomes
# <root>, the home directory ~, the host name <host> (stdin -> stdout).
fin_scrub() {
  local h hs root
  h="$(hostname -f 2> /dev/null || hostname)"
  hs="$(hostname -s 2> /dev/null || hostname)"
  root="${FINAL_ROOT%/}"
  sed -e "s|${root}|<root>|g" -e "s|${HOME}|~|g" -e "s|${h}|<host>|g" -e "s|\\b${hs}\\b|<host>|g"
}

# Scrub every text file under DIR in place (run at the end of a part, before
# its data can be copied into the repository).
fin_scrub_tree() {
  local dir=$1 f tmp
  while IFS= read -r -d '' f; do
    if LC_ALL=C grep -Iq . "$f" 2> /dev/null; then
      tmp="$(mktemp)"
      fin_scrub < "$f" > "$tmp" && cat "$tmp" > "$f"
      rm -f "$tmp"
    fi
  done < <(find "$dir" -type f -size -64M -print0)
}

# Machine-independent checks of a data directory before it is committed:
# no home directory, no host name, no IPv4 address. Prints the offending
# lines; status 1 if any.
fin_guard_tree() {
  local dir=$1 bad=0
  if grep -rIn -e "${HOME}" -e "/home/[a-z]" -e "/Users/[A-Za-z]" "$dir" | head -20 | grep .; then bad=1; fi
  if grep -rInw -e "$(hostname -s 2> /dev/null || hostname)" -e "$(hostname -f 2> /dev/null || hostname)" \
    -e '[a-z0-9-]*\.internal' "$dir" | head -5 | grep .; then bad=1; fi
  if grep -rInE '(^|[^0-9.])(25[0-5]|2[0-4][0-9]|1?[0-9]?[0-9])(\.(25[0-5]|2[0-4][0-9]|1?[0-9]?[0-9])){3}([^0-9.]|$)' \
    "$dir" --include='*.txt' --include='*.log' --include='*.md' --include='*.json' --include='*.jsonl' \
    | grep -v -e 'version' -e 'sha256' | head -5 | grep .; then bad=1; fi
  return "$bad"
}

# Machine description, toolchains and revisions (appended to DIR/provenance.txt
# once per session of a part).
fin_provenance() {
  local dir=$1 part=$2 v f
  {
    echo "== session $(fin_utc) part ${part}"
    echo "cpu=$(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2- | sed 's/^ *//')"
    echo "cores=$(lscpu -p=CORE 2> /dev/null | grep -v '^#' | sort -u | wc -l) cpus=$(nproc) threads_per_core=$(lscpu | sed -n 's/^Thread(s) per core: *//p')"
    echo "l3=$(lscpu | sed -n 's/^L3 cache: *//p') numa_nodes=$(lscpu | sed -n 's/^NUMA node(s): *//p')"
    echo "mem_total_kib=$(awk '/MemTotal/ {print $2}' /proc/meminfo) kernel=$(uname -r)"
    echo "os=$(. /etc/os-release 2> /dev/null && echo "${PRETTY_NAME}")"
    echo "docker=$(docker version --format '{{.Server.Version}}' 2> /dev/null)"
    for v in "${FIN_ORACLE_IMAGE}" "${FIN_MPI_IMAGE}" akantu-rs-mpi:dev; do
      echo "image ${v} $(docker image inspect --format '{{.Id}}' "$v" 2> /dev/null || echo missing)"
    done
    echo "cpp_reference=${FIN_CPP_REF}"
    echo "driver_checkout=$(git -C "${FIN_REPO}" rev-parse HEAD 2> /dev/null) dirty=$(git -C "${FIN_REPO}" status --porcelain 2> /dev/null | wc -l)"
    for f in "${FIN_BIN}"/*/VARIANT.json "${FIN_BIN}/cpp/CPP.json"; do
      [ -f "$f" ] && echo "$(basename "$(dirname "$f")"): $(tr -d '\n' < "$f")"
    done
    echo "mpirun=$(docker run --rm "${FIN_MPI_IMAGE}" mpirun --version 2> /dev/null | head -1)"
    echo "oracle_gcc=$(docker run --rm "${FIN_ORACLE_IMAGE}" g++ --version 2> /dev/null | head -1)"
    echo "python=$(python3 --version 2>&1)"
    echo "governor=$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2> /dev/null || echo n/a)"
  } | fin_scrub >> "${dir}/provenance.txt"
}

# Append a command line (scrubbed, with the UTC time) to DIR/commands.log.
fin_cmdlog() {
  local dir=$1; shift
  printf '%s %s\n' "$(fin_utc)" "$(printf '%q ' "$@")" | fin_scrub >> "${dir}/commands.log"
}

# One progress line per point (stdout and DIR/progress.log).
fin_progress() {
  local dir=$1; shift
  echo "$(fin_utc) $*" | tee -a "${dir}/progress.log"
}

# A failed check: recorded in DIR/CHECKS_FAILED and in the progress log; the
# part goes on with its other points and exits non-zero at the end.
fin_check_fail() {
  local dir=$1; shift
  echo "$(fin_utc) $*" >> "${dir}/CHECKS_FAILED"
  fin_progress "$dir" "CHECK FAILED: $*"
}

# Read one key of a variant's VARIANT.json (prints nothing if absent).
fin_variant_key() {
  python3 -I -c 'import json,sys
d=json.load(open(sys.argv[1]))
v=d.get(sys.argv[2], "")
print(json.dumps(v) if isinstance(v,(dict,list,bool)) else v)' "$1/VARIANT.json" "$2" 2> /dev/null
}

# Rotation: the ITEMS (words) rotated left by K places.
fin_rotate() {
  local k=$1; shift
  local items=("$@") n=$# i
  for ((i = 0; i < n; i++)); do printf '%s ' "${items[$(((i + k) % n))]}"; done
}

# Thread wrapper of a host binary: PROGDIR/NAME runs BIN with the given
# AKANTU_NUM_THREADS / AKANTU_PRECISION (and AKANTU_SPARSE_ORDERING when
# ORDERING is given, D-125), pinned to cores FIRST..FIRST+T-1 (one thread per
# physical core).
fin_wrapper() {
  local progdir=$1 name=$2 bin=$3 threads=$4 precision=$5 first=$6 ordering=${7:-}
  mkdir -p "$progdir"
  {
    echo '#!/bin/sh'
    echo "# ${name}: $(basename "$bin") with ${threads} thread(s), precision ${precision}"
    if [ "${precision}" = bitwise ]; then echo 'unset AKANTU_PRECISION'; else echo "AKANTU_PRECISION=${precision}; export AKANTU_PRECISION"; fi
    if [ "${threads}" = 1 ]; then echo 'unset AKANTU_NUM_THREADS'; else echo "AKANTU_NUM_THREADS=${threads}; export AKANTU_NUM_THREADS"; fi
    if [ -z "${ordering}" ]; then echo 'unset AKANTU_SPARSE_ORDERING'; else echo "AKANTU_SPARSE_ORDERING=${ordering}; export AKANTU_SPARSE_ORDERING"; fi
    echo "exec taskset -c ${first}-$((first + threads - 1)) \"${bin}\" \"\$@\""
  } > "${progdir}/${name}"
  chmod +x "${progdir}/${name}"
}

# The host's physical cores, one CPU (the first hardware thread) per core,
# ordered NUMA node by NUMA node, then by core id: comma list. Rank r of a
# run with T threads per rank gets the r-th block of T entries, so ranks fill
# one NUMA node before the next and a rank's threads sit on adjacent cores of
# one node (when T divides the node's core count).
fin_core_order() {
  lscpu -p=CPU,CORE,NODE 2> /dev/null | grep -v '^#' | sort -t, -k2,2n -k1,1n \
    | awk -F, '!seen[$2]++ {print ($3 == "" ? 0 : $3) "," $2 "," $1}' \
    | sort -t, -k1,1n -k2,2n | cut -d, -f3 | paste -sd,
}

# The first N entries of a comma list
fin_first() { echo "$2" | cut -d, -f"1-$1"; }

# Port variant labels of the timing parts: <base>[-fast][-t<N>] -> base, threads, precision
fin_parse_label() { # label -> "base threads precision"
  local l=$1 base threads=1 precision=bitwise
  base=${l%%-*}
  case "$l" in *-fast*) precision=fast ;; esac
  case "$l" in *-t[0-9]*) threads=${l##*-t} ;; esac
  echo "${base} ${threads} ${precision}"
}

# ---------------------------------------------------------------- sessions and groups
# One session per call of run_part.sh: its UTC start, the boot id and the CPU
# model (DIR/sessions.jsonl), so that a summary can mark the medians whose
# repetitions come from several sessions, and show a host change.
fin_session_start() { # dir -> sets FIN_SESSION
  local dir=$1
  FIN_SESSION="$(date -u +%Y%m%dT%H%M%SZ)"
  python3 -I - "$dir/sessions.jsonl" "$FIN_SESSION" <<'PY'
import json, platform, sys
out, sid = sys.argv[1:3]
def read(p):
    try:
        return open(p).read().strip()
    except OSError:
        return "unknown"
cpu = next((ln.split(":", 1)[1].strip() for ln in read("/proc/cpuinfo").splitlines()
            if ln.startswith("model name")), "unknown")
with open(out, "a") as f:
    f.write(json.dumps({"session": sid, "boot_id": read("/proc/sys/kernel/random/boot_id"),
                        "cpu": cpu, "kernel": platform.release()}) + "\n")
PY
}

# The outcome of one planned group (DIR/groups.jsonl): done | check_failed |
# failed | skipped; `done` and `skipped` also leave the marker that makes a
# later call skip the group.
fin_group_record() { # dir id status [note]
  local dir=$1 id=$2 st=$3 note=${4:-}
  python3 -I -c 'import json,sys; print(json.dumps({"id": sys.argv[1], "status": sys.argv[2], "session": sys.argv[3], "utc": sys.argv[4], "note": sys.argv[5]}))' \
    "$id" "$st" "${FIN_SESSION:-unknown}" "$(fin_utc)" "$note" >> "$dir/groups.jsonl"
  case "$st" in done|skipped) mkdir -p "$dir/.done"; echo "${FIN_SESSION:-unknown}" > "$dir/.done/$id" ;; esac
}

# Wait (at most FINAL_IDLE_WAIT seconds, default 600) until the machine is
# idle: no container of the campaign (name final-*) and less than half a core
# busy over one second (/proc/stat). Status 1 if it never was.
fin_wait_idle() {
  local waited=0 lim=${FINAL_IDLE_WAIT:-600} busy
  while :; do
    busy=$(python3 -I -c '
import time
def snap():
    f = [int(x) for x in open("/proc/stat").readline().split()[1:]]
    return sum(f), f[3] + f[4]
t0, i0 = snap(); time.sleep(1); t1, i1 = snap()
import os
print(f"{(1 - (i1 - i0) / max(t1 - t0, 1)) * os.cpu_count():.2f}")')
    if [ -z "$(docker ps -q --filter name=^final- 2> /dev/null)" ] && awk -v b="$busy" 'BEGIN { exit !(b < 0.5) }'; then
      return 0
    fi
    [ "$waited" -lt "$lim" ] || { echo "busy=${busy} cores, containers: $(docker ps --format '{{.Names}}' | paste -sd, -)"; return 1; }
    sleep 15; waited=$((waited + 15))
  done
}

# Check every binary of a variant directory against its VARIANT.json (and the
# Python extension against python_extension_sha256): prints the mismatches,
# status 1 if any.
fin_verify_variant() { # dir
  python3 -I - "$1" <<'PY'
import glob, hashlib, json, os, sys
d = sys.argv[1]
v = json.load(open(os.path.join(d, "VARIANT.json")))
bad = []
def sha(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()
for rel, want in v.get("sha256", {}).items():
    p = os.path.join(d, rel)
    if not os.path.exists(p):
        bad.append(f"{rel}: missing")
    elif sha(p) != want:
        bad.append(f"{rel}: SHA-256 {sha(p)[:16]} differs from VARIANT.json {want[:16]}")
ext = v.get("python_extension_sha256")
if ext:
    so = glob.glob(os.path.join(d, "venv", "lib", "python*", "site-packages", "akantu", "*.so"))
    if not so or sha(so[0]) != ext:
        bad.append("the Python extension differs from VARIANT.json")
for b in bad:
    print(f"{d}: {b}")
sys.exit(1 if bad else 0)
PY
}

# The cores of one run of N ranks x T threads: `fill` takes the first N x T
# entries of CORE_ORDER (one NUMA node after the other); `spread` takes the
# same number of cores from every NUMA node (N divisible by the node count),
# each node's in core order, so that 64 ranks on two nodes run as 32 + 32.
fin_run_cores() { # ranks threads placement core_order
  python3 -I - "$1" "$2" "$3" "$4" <<'PY'
import os, sys
n, t, placement, order = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3], sys.argv[4].split(",")
node = {}
base = "/sys/devices/system/node"
if os.path.isdir(base):
    for d in os.listdir(base):
        if d.startswith("node") and d[4:].isdigit():
            for part in open(f"{base}/{d}/cpulist").read().strip().split(","):
                a, _, b = part.partition("-")
                for c in range(int(a), int(b or a) + 1):
                    node[str(c)] = int(d[4:])
nodes = sorted({node.get(c, 0) for c in order})
if placement == "spread" and len(nodes) > 1 and n % len(nodes) == 0:
    per = n // len(nodes) * t
    out = []
    for k in nodes:
        out += [c for c in order if node.get(c, 0) == k][:per]
else:
    out = order[:n * t]
print(",".join(out))
PY
}
