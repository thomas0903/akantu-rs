#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs MPI test support (no upstream counterpart).
# Repetition campaign of the launcher teardown race, run INSIDE the MPI image as the
# akantu user (build the binaries and the shim as run-mpi-tier.sh does;
# for gdb backtraces run the container with --cap-add SYS_PTRACE and give
# gdb the file capability: setcap cap_sys_ptrace+eip /usr/bin/gdb).
# Repeats the MPI tier's exact invocation of
#   mpi_abort_paths worker_abort_funneled
# at 8 ranks (timeout -k 10 120, shim LD_PRELOADed), records rc / seconds,
# and captures launcher diagnostics for every run still alive after HANG_S.
#
#   repeat-abort-scenario.sh <outdir> <runs> <label> [extra mpirun args...]
#
# Env: ABORT_BIN, SHIM (paths), NP (default 8), SCENARIO (default
# worker_abort_funneled), HANG_S (default 30), NO_SHIM=1 to omit LD_PRELOAD.
# Adversarial PMIx_Abort interposers (scripts/mpi/pmix_abort_shims.c):
# PMIX_SHIM=<interposer.so> with
# PMIX_FORM =
#   all       the interposer on every rank (no rank makes the real request,
#             every rank _exits at once: meets the PRRTE teardown race;
#             kept here only, for campaigns, not in the MPI tier);
#   one       the interposer on rank 0 only, the other NP-1 ranks with SHIM
#             only (preload-rank0.sh: the MPI tier's form at N >= 2 ranks;
#             it stands in for the MPMD form, which the image's prterun
#             3.0.8 cannot launch);
#   singleton one process without a launcher (the MPI tier's form at 1
#             rank; NP is ignored).
# results.tsv: run, rc, seconds, hung, evidence (check_evidence.py exit
# status), calls (interposer calls in the run's stderr; 0 without
# PMIX_SHIM).
set -u
here="$(cd "$(dirname "$0")" && pwd)"
out=$1; runs=$2; label=$3; shift 3
np=${NP:-8}
scenario=${SCENARIO:-worker_abort_funneled}
hang_s=${HANG_S:-30}
pmix_shim=${PMIX_SHIM:-}
pmix_form=${PMIX_FORM:-all}
dir="$out/$label"
mkdir -p "$dir"
res="$dir/results.tsv"
printf 'run\trc\tseconds\thung\tevidence\tcalls\n' > "$res"
preload=(-x "LD_PRELOAD=$SHIM")
[ "${NO_SHIM:-0}" = 1 ] && preload=()
# the command of one run (array `cmd`), per form
check_np=$np
if [ -z "$pmix_shim" ]; then
  cmd=(mpirun "$@" ${preload[@]+"${preload[@]}"} -np "$np" "$ABORT_BIN" "$scenario")
else
  case "$pmix_form" in
    all) cmd=(mpirun "$@" -x "LD_PRELOAD=$SHIM:$pmix_shim" -np "$np" "$ABORT_BIN" "$scenario") ;;
    one)
      [ "$np" -ge 2 ] || { echo "PMIX_FORM=one needs NP >= 2" >&2; exit 2; }
      cmd=(mpirun "$@" -x "AKANTU_TEST_PRELOAD=$SHIM" -x "AKANTU_TEST_PRELOAD_RANK0=$pmix_shim"
        -np "$np" sh "$here/preload-rank0.sh" "$ABORT_BIN" "$scenario")
      ;;
    singleton) check_np=1; cmd=(env "LD_PRELOAD=$SHIM:$pmix_shim" "$ABORT_BIN" "$scenario") ;;
    *) echo "unknown PMIX_FORM '$pmix_form' (all, one, singleton)" >&2; exit 2 ;;
  esac
fi
{
  echo "label=$label np=$np scenario=$scenario hang_s=$hang_s pmix_shim=${pmix_shim:-none} pmix_form=$pmix_form extra=$*"
  echo "command=${cmd[*]}"
  echo "mpirun=$(mpirun --version | head -1)"
  echo "kernel=$(uname -r) nproc=$(nproc)"
  env | grep -E '^OMPI_MCA_|^PRTE_MCA_|^PMIX_MCA_' | sort
} > "$dir/provenance.txt"

capture() { # run-index timeout-pid
  local i=$1 tp=$2 d="$dir/hang$1"
  mkdir -p "$d"
  ps -eo pid,ppid,pgid,stat,wchan:40,etimes,args > "$d/ps.txt" 2>&1
  # the launcher: the mpirun/prterun child of `timeout`
  local p
  for p in $(pgrep -P "$tp"); do
    {
      echo "== pid $p: $(tr '\0' ' ' < "/proc/$p/cmdline")"
      for t in "/proc/$p"/task/*; do
        echo "-- task ${t##*/} $(cat "$t/comm") state=$(awk '{print $3}' "$t/stat") wchan=$(cat "$t/wchan" 2>/dev/null)"
      done
      ls -l "/proc/$p/fd" 2>&1
    } >> "$d/launcher.txt"
    if command -v gdb >/dev/null; then
      timeout 60 gdb -p "$p" -batch -ex 'set pagination off' \
        -ex 'info threads' -ex 'thread apply all bt full' > "$d/gdb-$p.txt" 2>&1
    fi
    # its children (daemons, if any) and any surviving ranks
    for c in $(pgrep -P "$p"); do
      echo "child $c: $(tr '\0' ' ' < "/proc/$c/cmdline" 2>/dev/null) state=$(awk '{print $3}' "/proc/$c/stat" 2>/dev/null)" >> "$d/launcher.txt"
    done
  done
}

for i in $(seq 1 "$runs"); do
  o="$dir/run$i.out"; e="$dir/run$i.err"
  t0=$(date +%s.%N)
  timeout -k 10 120 "${cmd[@]}" > "$o" 2> "$e" &
  tp=$!
  hung=0
  while kill -0 "$tp" 2>/dev/null; do
    el=$(python3 -c "import time; print(int(time.time() - $t0))")
    if [ "$hung" = 0 ] && [ "$el" -ge "$hang_s" ]; then
      hung=1
      capture "$i" "$tp"
    fi
    sleep 0.2
  done
  wait "$tp"; rc=$?
  t1=$(date +%s.%N)
  secs=$(python3 -c "print(f'{$t1-$t0:.1f}')")
  python3 "$here/check_evidence.py" "$scenario" "$check_np" "$o" "$e" > "$dir/run$i.check" 2>&1
  ev=$?
  calls=0
  [ -z "$pmix_shim" ] || calls=$(grep -ac 'PMIX_SHIM_[A-Z]* PMIx_Abort(' "$e")
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$i" "$rc" "$secs" "$hung" "$ev" "$calls" >> "$res"
  # keep the logs of abnormal runs only
  if [ "$hung" = 0 ] && [ "$rc" = 71 ] && [ "$ev" = 0 ]; then rm -f "$o" "$e" "$dir/run$i.check"; fi
done
total=$(($(wc -l < "$res") - 1))
ok=$(awk -F'\t' 'NR>1 && $2==71 && $3<30' "$res" | wc -l)
hangs=$(awk -F'\t' 'NR>1 && $4==1' "$res" | wc -l)
evid=$(awk -F'\t' 'NR>1 && $5==0' "$res" | wc -l)
called=$(awk -F'\t' 'NR>1 && $6>=1' "$res" | wc -l)
echo "SUMMARY label=$label runs=$total pass(rc71,<30s)=$ok hangs=$hangs evidence_ok=$evid interposer_reached=$called" | tee "$dir/summary.txt"
