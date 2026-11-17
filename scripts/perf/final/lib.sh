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
FIN_BASELINE="${FINAL_BASELINE:-${HOME}/baseline}"
FIN_TVOL="${FINAL_TARGET_VOLUME:-akantu-rs-mpi-target}"
FIN_MPI_IMAGE="${FINAL_MPI_IMAGE:-akantu-rs-mumps:dev}"
FIN_ORACLE_IMAGE="${FINAL_ORACLE_IMAGE:-akantu-oracle:dev}"
# the C++ reference commit (docs/REFERENCE.md)
FIN_CPP_REF=35a851780d0b9471d258ed53a003ae98aaa95ea0

fin_utc() { date -u +%FT%TZ; }
fin_die() { echo "$(fin_utc) FATAL: $*" >&2; exit 2; }

# Rewrite a text so that it names no machine: the data root becomes
# <root>, the home directory ~, the host name <host> (stdin -> stdout).
fin_scrub() {
  local h root
  h="$(hostname 2> /dev/null || echo unknown-host)"
  root="${FINAL_ROOT%/}"
  sed -e "s|${root}|<root>|g" -e "s|${HOME}|~|g" -e "s|\\b${h}\\b|<host>|g"
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
  local dir=$1 h bad=0
  h="$(hostname 2> /dev/null || echo unknown-host)"
  if grep -rIn -e "${HOME}" -e "/home/[a-z]" -e "/Users/[A-Za-z]" "$dir" | head -20 | grep .; then bad=1; fi
  if grep -rInw -e "$h" "$dir" | head -5 | grep .; then bad=1; fi
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
# AKANTU_NUM_THREADS / AKANTU_PRECISION, pinned to cores FIRST..FIRST+T-1
# (one thread per physical core).
fin_wrapper() {
  local progdir=$1 name=$2 bin=$3 threads=$4 precision=$5 first=$6
  mkdir -p "$progdir"
  {
    echo '#!/bin/sh'
    echo "# ${name}: $(basename "$bin") with ${threads} thread(s), precision ${precision}"
    if [ "${precision}" = bitwise ]; then echo 'unset AKANTU_PRECISION'; else echo "AKANTU_PRECISION=${precision}; export AKANTU_PRECISION"; fi
    if [ "${threads}" = 1 ]; then echo 'unset AKANTU_NUM_THREADS'; else echo "AKANTU_NUM_THREADS=${threads}; export AKANTU_NUM_THREADS"; fi
    echo "exec taskset -c ${first}-$((first + threads - 1)) \"${bin}\" \"\$@\""
  } > "${progdir}/${name}"
  chmod +x "${progdir}/${name}"
}

# Port variant labels of the timing parts: <base>[-fast][-t<N>] -> base, threads, precision
fin_parse_label() { # label -> "base threads precision"
  local l=$1 base threads=1 precision=bitwise
  base=${l%%-*}
  case "$l" in *-fast*) precision=fast ;; esac
  case "$l" in *-t[0-9]*) threads=${l##*-t} ;; esac
  echo "${base} ${threads} ${precision}"
}
