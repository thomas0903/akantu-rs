#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): one part of the final
# measurement campaign, unattended and resumable
# (scripts/perf/final/README.md).
#
#   scripts/perf/final/run_part.sh PART [--reps N] [--size dry|full] [options]
#
#   PART       A  the explicit step on one core (ten configurations, both C++
#                 builds, the port before and at head, threads, fast mode)
#              B  the benchmark suites (suite_solid, suite_models, suite_setup,
#                 suite_python.py): serial cases, threads and fast mode where
#                 they can change something, the distributed cases' rank series
#              C  the cohesive benchmark at small and mid sizes, 1 to 8 ranks
#              D  static solves: SparseLU against upstream's, MUMPS at 1/2/4
#                 ranks, the solver phases of the suite
#              E  memory: peak resident set per rank and summed, with the
#                 option that releases the static solver's factors
#              F  set-up phases, the distribution at size l, the MPI layer
#              G  the full-size run (JOSS mesh; one 128-core node)
#   --reps N      repetitions (default 5; part G: 1)
#   --size S      dry (the smallest sizes: a development check of the whole
#                 chain, not a result) or full (the sizes of the published
#                 tables; default)
#   --only LIST   comma list of the part's series (default all; to split a
#                 part over machines, see README.md)
#   --variants P  comma list of shell patterns: only the matching variants run
#                 (e.g. `cpp,head`, `before,head*`)
#   --plan-only   print the plan (groups, runs and expected time per series,
#                 total; part G also memory per run) and exit; nothing runs.
#                 FINAL_PLAN_CORES=128 FINAL_PLAN_MEMORY=481g: another machine
#   --root DIR    FINAL_ROOT (default ~/final): binaries in DIR/bin, data in DIR/data
#   --data DIR    data root (default FINAL_ROOT/data); the part writes DIR/<PART>
#   --before DIR, --head DIR   prepared variant directories (default
#                 FINAL_ROOT/bin/before, FINAL_ROOT/bin/head; prepare.sh)
#   --pin CORE    the core of the serial runs (default 2); N threads on cores
#                 0..N-1; ranks on the host's cores ordered NUMA node by node
#   --memory M    container memory limit of the cohesive runs, no swap
#                 (default: the machine's memory minus 3 GiB; part G 94 %)
#   --points "P..."  part G: the runs in their order, P = RANKS[xTHREADS]:VARIANT[,VARIANT]
#                 (default "128:head 128:cpp 64:head 64:cpp 32:head 32:cpp 64x2:head
#                 32x4:head 16x8:head 8:head"; dry "8:head 8:cpp 4x2:head")
#   --placement fill|spread   part G: fill one NUMA node before the next
#                 (default) or put the same number of ranks on every node
#   --sparselu-threads auto|true|false   part D: time the threaded SparseLU
#                 variants (default auto: what prepare.sh measured, VARIANT.json)
#
# The plan (plan.py; DATA/<PART>/plan.jsonl) lists every group in execution
# order: the variants of one configuration in one repetition, their order
# rotated by the repetition. A group that completes and passes its checks
# leaves a marker; a second call skips it and repeats the others. Every
# outcome goes to DATA/<PART>/groups.jsonl with the session (sessions.jsonl:
# UTC start, boot id, CPU), one progress line per group to progress.log.
# Checks: the binaries against VARIANT.json / CPP.json (at the start: the part
# stops; per cohesive run: the run fails its check), state digests of every
# bitwise variant, thread count and precision recorded by every run, the core
# binding of every MPI run, upstream's MUMPS retry path. Before each group the
# machine must be idle (no final-* container, less than half a core busy).
# Exit status: 0 if every group is complete and passed, 1 otherwise, 2 for a
# usage error or a binary that is not the one its directory names.
set -uo pipefail
# shellcheck source=scripts/perf/final/lib.sh
. "$(dirname "$0")/lib.sh"

part=${1:-}
[ -n "$part" ] || { sed -n '6,62p' "$0"; exit 2; }
shift
reps="" size=full only="" data="" pin=2 memory="" before="" head="" gpoints="" planonly=0
variants="" placement=fill slu=auto
while [ $# -gt 0 ]; do
  case "$1" in
    --reps) reps=${2:?}; shift 2 ;;
    --size) size=${2:?}; shift 2 ;;
    --only) only=${2:?}; shift 2 ;;
    --variants) variants=${2:?}; shift 2 ;;
    --root) FINAL_ROOT=${2:?}; shift 2 ;;
    --data) data=${2:?}; shift 2 ;;
    --before) before=${2:?}; shift 2 ;;
    --head) head=${2:?}; shift 2 ;;
    --pin) pin=${2:?}; shift 2 ;;
    --memory) memory=${2:?}; shift 2 ;;
    --points) gpoints=${2:?}; shift 2 ;;
    --placement) placement=${2:?}; shift 2 ;;
    --sparselu-threads) slu=${2:?}; shift 2 ;;
    --plan-only) planonly=1; shift ;;
    *) fin_die "run_part.sh: unknown argument $1" ;;
  esac
done
case "$part" in A|B|C|D|E|F|G) ;; *) fin_die "run_part.sh: unknown part ${part}" ;; esac
case "$size" in dry|full) ;; *) fin_die "--size dry|full" ;; esac
case "$placement" in fill|spread) ;; *) fin_die "--placement fill|spread" ;; esac
case "$slu" in auto|true|false) ;; *) fin_die "--sparselu-threads auto|true|false" ;; esac
[ -n "$reps" ] || { if [ "$part" = G ]; then reps=1; else reps=5; fi; }
[[ "$reps" =~ ^[1-9][0-9]*$ ]] || fin_die "--reps: a positive integer"
FIN_BIN="${FINAL_ROOT}/bin"
BEFORE="${before:-${FIN_BIN}/before}"
HEAD="${head:-${FIN_BIN}/head}"
NCORES=$(lscpu -p=CORE 2> /dev/null | grep -v '^#' | sort -u | wc -l)
[ "$NCORES" -ge 1 ] || NCORES=$(nproc)
if [ -z "$memory" ]; then
  memtot=$(( $(awk '/MemTotal/ {print $2}' /proc/meminfo) / 1048576 ))
  if [ "$part" = G ]; then memory="$(( memtot * 94 / 100 ))g"; else memory="$(( memtot - 3 ))g"; fi
fi
if [ "$slu" = auto ]; then
  slu=$(fin_variant_key "$HEAD" sparselu_threaded 2> /dev/null)
  [ "$slu" = true ] || slu=false
fi
plan_args=("$part" --size "$size" --reps "$reps" --only "$only" --variants "$variants"
  --points "$gpoints" --placement "$placement" --sparselu-threaded "$slu"
  --before "$BEFORE" --head "$HEAD")

if [ "$planonly" = 1 ]; then
  exec python3 -I "${FIN_HERE}/plan.py" "${plan_args[@]}" --cores "${FINAL_PLAN_CORES:-$NCORES}" \
    --memory "${FINAL_PLAN_MEMORY:-$memory}" --print
fi

[ -f "${BEFORE}/VARIANT.json" ] || fin_die "no prepared variant ${BEFORE} (prepare.sh)"
[ -f "${HEAD}/VARIANT.json" ] || fin_die "no prepared variant ${HEAD} (prepare.sh)"
D="${data:-${FINAL_ROOT}/data}/${part}"
mkdir -p "$D"
D="$(cd "$D" && pwd)"
# the host's cores NUMA node by NUMA node (lib.sh): runs take blocks of it
CORES_ORDER="${FINAL_CORE_ORDER:-$(fin_core_order)}"
[ -n "$CORES_ORDER" ] || CORES_ORDER="$(seq -s, 0 $((NCORES - 1)))"
CAMPAIGN_TOOL="${FIN_REPO}/crates/akantu-bench/scripts/c1prep_campaign.sh"
X="${FIN_REPO}/scripts/perf/explicit-step"
exec 8> "${D}/.lock"
flock -n 8 || fin_die "another run_part.sh holds ${D}/.lock"

# failures of an earlier session are kept as history: this session repeats
# every incomplete group and reports its own outcome
for f in FAILED CHECKS_FAILED; do
  if [ -s "$D/$f" ]; then mkdir -p "$D/history"; mv "$D/$f" "$D/history/$f.$(date -u +%Y%m%dT%H%M%SZ)"; fi
done
fin_session_start "$D"
fin_provenance "$D" "$part"
fin_cmdlog "$D" "$0" "$part" --reps "$reps" --size "$size" ${only:+--only "$only"} ${variants:+--variants "$variants"} \
  --pin "$pin" --memory "$memory" --placement "$placement" --sparselu-threads "$slu"
{
  echo "part=${part} size=${size} reps=${reps} only=${only:-all} variants=${variants:-all} pin=${pin} memory=${memory} cores=${NCORES} placement=${placement} session=${FIN_SESSION}"
  echo "before=$(fin_variant_key "$BEFORE" revision) head=$(fin_variant_key "$HEAD" revision) cpp=${FIN_CPP_REF}"
  echo "sparselu_threaded=${slu} (head: $(fin_variant_key "$HEAD" sparselu_probe))"
  echo "core_order=${CORES_ORDER} (one CPU per physical core, NUMA node by NUMA node)"
  lscpu | grep -E '^NUMA node[0-9]+ CPU' | sed 's/^/numa: /'
} | fin_scrub >> "$D/provenance.txt"

# ---------------------------------------------------------------- the binaries
# every binary of both variants is the one its VARIANT.json names; the C++
# side is the one CPP.json names; the port's MPI driver runs from a copy in
# the Docker volume addressed by its SHA-256 (never a path another FINAL_ROOT
# or another prepare.sh can overwrite)
for v in "$BEFORE" "$HEAD"; do
  fin_verify_variant "$v" > "$D/.verify" 2>&1 || { cat "$D/.verify" >&2; fin_die "the binaries of $v are not those of its VARIANT.json"; }
done
CPP_JSON="${FIN_BIN}/cpp/CPP.json"
cpp_sha() { python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["sha256"].get(sys.argv[2], ""))' "$CPP_JSON" "$1" 2> /dev/null; }
declare -A RSHA RBIN
case "$part" in
  C|D|E|F|G)
    for c in strict default; do
      got=$(docker run --rm -v akantu-perf-cohesive:/perf:ro "${FIN_ORACLE_IMAGE}" sha256sum "/perf/builds/$c/cohesive_extrinsic" 2> /dev/null | cut -d' ' -f1)
      [ -n "$got" ] && [ "$got" = "$(cpp_sha "cohesive_extrinsic_$c")" ] \
        || fin_die "the C++ harness /perf/builds/$c is not the one of CPP.json (got ${got:-none})"
    done
    for b in before head; do
      vd=$(if [ "$b" = before ]; then echo "$BEFORE"; else echo "$HEAD"; fi)
      sha=$(sha256sum "$vd/mpi/cohesive_extrinsic" | cut -d' ' -f1)
      path="/target/final/sha/$sha/cohesive_extrinsic"
      got=$(docker run --rm -u 0 -v "${FIN_TVOL}:/target" -v "$vd/mpi:/src:ro" "${FIN_MPI_IMAGE}" sh -c \
        "test -x $path || { mkdir -p /target/final/sha/$sha && cp /src/cohesive_extrinsic $path && chmod -R a+rX /target/final; }; sha256sum $path" | cut -d' ' -f1)
      [ "$got" = "$sha" ] || fin_die "the copy of ${b}'s driver in the volume ($path) has SHA-256 ${got:-none}"
      RSHA[$b]=$sha; RBIN[$b]=$path
      echo "driver_${b}=${path}" >> "$D/provenance.txt"
    done ;;
  A)
    for c in strict default; do
      [ "$(sha256sum "${FIN_BIN}/cpp/explicit_step_timing_$c" | cut -d' ' -f1)" = "$(cpp_sha "explicit_step_timing_$c")" ] \
        || fin_die "${FIN_BIN}/cpp/explicit_step_timing_$c is not the one of CPP.json"
    done ;;
esac

# ---------------------------------------------------------------- the plan
python3 -I "${FIN_HERE}/plan.py" "${plan_args[@]}" --cores "$NCORES" --memory "$memory" --out "$D" --print \
  > "$D/plan.txt" || fin_die "plan.py failed"
fin_progress "$D" "part ${part} start (session ${FIN_SESSION}): $(grep -c . "$D/plan.tsv") groups, size ${size}, ${reps} repetitions${only:+, series ${only}}${variants:+, variants ${variants}}"
fin_progress "$D" "plan: $(grep '^total' "$D/plan.txt" | tr -s ' ')"

# variant label -> host directory of its base variant
vdir() { case "${1%%-*}" in before) echo "$BEFORE" ;; head) echo "$HEAD" ;; *) fin_die "variant $1" ;; esac; }
threads_of() { case "$1" in *-t[0-9]*) echo "${1##*-t}" ;; *) echo 1 ;; esac; }
precision_of() { case "$1" in *-fast*) echo fast ;; *) echo bitwise ;; esac; }

# ----------------------------------------------------------------------------
# Part A: the explicit step
a_wrappers() { # labels... -> wrappers in $D/progs
  local l t p
  for l in "$@"; do
    case "$l" in cpp-*) continue ;; esac
    t=$(threads_of "$l"); p=$(precision_of "$l")
    if [ "$t" = 1 ]; then fin_wrapper "$D/progs" "$l" "$(vdir "$l")/explicit_step_timing" 1 "$p" "$pin"
    else fin_wrapper "$D/progs" "$l" "$(vdir "$l")/explicit_step_timing" "$t" "$p" 0; fi
  done
}

run_a_digests() { # id labels configs
  local id=$1 cfgs=$3 labels l c mesh mode n fd rp rs rdd ok=1 dig ref verdict
  read -r -a labels <<< "$2"
  a_wrappers "${labels[@]}"
  local meshes="${FINAL_ROOT}/meshes/explicit" rec="${FIN_REPO}/docs/benchmarks/explicit-step/final/digests.txt"
  : > "$D/digests.tsv"
  for c in $cfgs; do
    IFS=: read -r mesh _ fd <<< "$c"; mode=ss; [ "${fd:-}" = fd ] && mode=fd
    n=$(awk -v m="$mesh" -v o="$mode" '$1 == m && $2 == o {sub("n=", "", $3); print $3}' "$rec")
    read -r rp rs rdd <<< "$(awk -v m="$mesh" -v o="$mode" '$1 == m && $2 == o {print $4, $7, $8}' "$rec")"
    ref=""
    for l in "${labels[@]}"; do
      case "$l" in
        cpp-*) dig=$(PIN_CPU="$pin" "$X/cpp_run.sh" "${FIN_BIN}/cpp" "${l#cpp-}" "$meshes" "$mesh" 1 1 0 \
                 "$([ "$mode" = fd ] && echo 1 || echo 0)" check "$n" | sed -n 's/.*"fnv1a":"\([0-9a-f]*\)".*/\1/p') ;;
        *) local fdarg=(); [ "$mode" = fd ] && fdarg=(--fd)
           dig=$("$D/progs/$l" --mesh "$meshes/$mesh.msh" --digest-steps "$n" "${fdarg[@]}" | sed -n 's/.*"fnv1a":"\([0-9a-f]*\)".*/\1/p') ;;
      esac
      case "$l" in
        *-fast*) verdict="not-compared(fast)" ;;
        cpp-default) if [ "$dig" = "$rdd" ]; then verdict="=recorded"; else verdict="DIFFERS(recorded ${rdd})"; ok=0; fi ;;
        *) [ -n "$ref" ] || ref=$dig
           if [ -n "$dig" ] && [ "$dig" = "$ref" ] && [ "$dig" = "$rs" ] && [ "$dig" = "$rp" ]; then verdict="=cpp-strict=recorded"
           else verdict="DIFFERS(recorded ${rs})"; ok=0; fi ;;
      esac
      printf '%s\t%s\tn=%s\t%s\t%s\t%s\n' "$mesh" "$mode" "$n" "$l" "${dig:-none}" "$verdict" >> "$D/digests.tsv"
    done
  done
  if [ "$ok" = 1 ]; then fin_group_record "$D" "$id" done; fin_progress "$D" "A digests: all equal (digests.tsv)"
  else fin_group_record "$D" "$id" check_failed; fin_check_fail "$D" "A digests: a bitwise program differs (digests.tsv)"; fi
}

run_a_round() { # id rep labels configs
  local id=$1 r=$2 cfgs=$4 labels
  read -r -a labels <<< "$3"
  a_wrappers "${labels[@]}"
  local meshes="${FINAL_ROOT}/meshes/explicit"
  mkdir -p "$D/rounds"
  rm -f "$D/rounds/r$r.jsonl"
  # the plan's order is the round's order: timing_rotated.sh rotates by the
  # round number, so give it the list rotated back
  local n=${#labels[@]} back
  read -r -a back <<< "$(fin_rotate "$(( (n - (r - 1) % n) % n ))" "${labels[@]}")"
  fin_cmdlog "$D" env ROUND_START="$r" PIN_CPU="$pin" BIN_DIR="$D/progs" CPP_BIN_DIR="${FIN_BIN}/cpp" MESH_DIR="$meshes" \
    "$X/timing_rotated.sh" "$D/rounds/r$r.jsonl" 1 "${back[*]}" "$cfgs"
  if env ROUND_START="$r" PIN_CPU="$pin" BIN_DIR="$D/progs" CPP_BIN_DIR="${FIN_BIN}/cpp" MESH_DIR="$meshes" \
    "$X/timing_rotated.sh" "$D/rounds/r$r.jsonl" 1 "${back[*]}" "$cfgs" > "$D/rounds/r$r.log" 2>&1; then
    if check_a_round "$D/rounds/r$r.jsonl" "${labels[@]}"; then
      fin_group_record "$D" "$id" done; fin_progress "$D" "A round ${r}: done"
    else fin_group_record "$D" "$id" check_failed; fin_progress "$D" "A round ${r}: CHECK FAILED"; fi
  else
    fin_group_record "$D" "$id" failed; echo "$(fin_utc) $id" >> "$D/FAILED"; fin_progress "$D" "A round ${r}: FAILED (rounds/r$r.log)"
  fi
}

# every program's time lines present and every port meta line with the
# requested threads and precision (of the process default policy, read from
# the environment by the harness)
check_a_round() {
  local f=$1; shift
  python3 -I - "$f" "$D/CHECKS_FAILED" "$@" <<'PY'
import json, sys
f, failed, labels = sys.argv[1], sys.argv[2], sys.argv[3:]
seen, bad = {}, []
for line in open(f):
    if not line.startswith("{"):
        continue
    o = json.loads(line)
    if o.get("kind") == "time":
        seen[o["label"].split("|")[0]] = seen.get(o["label"].split("|")[0], 0) + 1
    if o.get("kind") != "meta":
        continue
    lab = o["label"].split("|")[0]
    if lab.startswith("cpp-"):
        continue
    want_t = int(lab.rsplit("-t", 1)[1]) if "-t" in lab else 1
    want_p = "fast" if "-fast" in lab else "bitwise"
    got_t, got_p = o.get("threads", 1), str(o.get("precision", "bitwise")).lower()
    if got_t != want_t or got_p != want_p:
        bad.append(f"{o['label']}: threads {got_t} precision {got_p}, requested {want_t} {want_p}")
missing = [l for l in labels if not seen.get(l)]
if missing:
    bad.append(f"no time lines for {missing}")
if bad:
    with open(failed, "a") as h:
        for b in bad:
            h.write(f"A {f}: {b}\n")
    print("\n".join(bad), file=sys.stderr)
    sys.exit(1)
PY
}

# ----------------------------------------------------------------------------
# Suites (parts B, D, F)

# serial group: one case, one repetition, the variants in the plan's order (run_ab.sh)
run_suite() { # id rep labels dir bin case size
  local id=$1 r=$2 out="$D/$4" bin=$5 c=$6 sz=$7 labels l args=() t p
  read -r -a labels <<< "$3"
  mkdir -p "$out/progs"
  for l in "${labels[@]}"; do
    t=$(threads_of "$l"); p=$(precision_of "$l")
    if [ "$t" = 1 ]; then fin_wrapper "$out/progs" "$l" "$(vdir "$l")/$bin" 1 "$p" "$pin"
    else fin_wrapper "$out/progs" "$l" "$(vdir "$l")/$bin" "$t" "$p" 0; fi
  done
  # run_ab.sh rotates its variants by the repetition: give it the plan's order rotated back
  local n=${#labels[@]} back
  read -r -a back <<< "$(fin_rotate "$(( (n - r % n) % n ))" "${labels[@]}")"
  for l in "${back[@]}"; do args+=("$l=$out/progs/$l"); done
  fin_cmdlog "$D" "${FIN_REPO}/scripts/perf/suite/run_ab.sh" --out "$out" --size "$sz" --reps 1 --rep-offset "$r" \
    --cases "$c" --core "$pin" "${args[@]}"
  if "${FIN_REPO}/scripts/perf/suite/run_ab.sh" --out "$out" --size "$sz" --reps 1 --rep-offset "$r" \
    --cases "$c" --core "$pin" "${args[@]}" > "$out/.log.$c.r$r" 2>&1; then
    if check_suite_group "$out" "$c" "$r" "" "$out/.log.$c.r$r" "${labels[@]}"; then
      fin_group_record "$D" "$id" done; fin_progress "$D" "$id: done"
    else fin_group_record "$D" "$id" check_failed; fin_progress "$D" "$id: CHECK FAILED"; fi
  elif grep -q "${c}: skip:" "$out/.log.$c.r$r"; then
    grep -m1 "${c}: skip:" "$out/.log.$c.r$r" >> "$out/.skipped"
    fin_group_record "$D" "$id" skipped "$(grep -m1 "${c}: skip:" "$out/.log.$c.r$r")"; fin_progress "$D" "$id: skipped (this build)"
  else
    fin_group_record "$D" "$id" failed; echo "$(fin_utc) $id" >> "$D/FAILED"; fin_progress "$D" "$id: FAILED ($out/.log.$c.r$r)"
  fi
}

# MPI group: one case at P ranks (x T threads), one repetition; every rank
# bound to its core of the NUMA-ordered list, the binding recorded and checked
run_suite_mpi() { # id rep labels dir bin case size np
  local id=$1 r=$2 out="$D/$4" bin=$5 c=$6 sz=$7 np=$8 labels l rc=0 t p bad=0
  read -r -a labels <<< "$3"
  for l in "${labels[@]}"; do
    t=$(threads_of "$l"); p=$(precision_of "$l")
    local f="$out/$l/np$np/$c.r$r.jsonl" envs=() mapping=(--map-by core --bind-to core) cores rec
    mkdir -p "$(dirname "$f")"
    cores=$(fin_run_cores "$np" "$t" fill "$CORES_ORDER")
    [ "$t" = 1 ] || { envs+=(-e "AKANTU_NUM_THREADS=$t"); mapping=(--map-by "slot:PE=$t" --bind-to core -x AKANTU_NUM_THREADS); }
    [ "$p" = bitwise ] || { envs+=(-e "AKANTU_PRECISION=$p"); mapping+=(-x AKANTU_PRECISION); }
    local cmd=(docker run --rm --init --name "final-suite-$$-$RANDOM" --cpuset-cpus "$cores" --shm-size 2g -e SCOTCH_PTHREAD_NUMBER=1
      -e OMP_NUM_THREADS=1 -e OPENBLAS_NUM_THREADS=1 ${envs[@]+"${envs[@]}"} -v "$(vdir "$l")/mpi:/bench:ro" "${FIN_MPI_IMAGE}"
      mpirun --allow-run-as-root -n "$np" "${mapping[@]}" -x SCOTCH_PTHREAD_NUMBER -x OMP_NUM_THREADS -x OPENBLAS_NUM_THREADS
      "/bench/$bin" --case "$c" --size "$sz" --reps 1)
    fin_cmdlog "$D" "${cmd[@]}"
    python3 -I "${FIN_HERE}/bindings.py" record "${f%.jsonl}.bindings.json" "$np" --comm "${bin:0:15}" > /dev/null 2>&1 &
    rec=$!
    local t0 t1
    t0=$(date +%s)
    "${cmd[@]}" > "$f" 2> "${f%.jsonl}.err" || rc=1
    t1=$(date +%s)
    kill "$rec" 2> /dev/null; wait "$rec" 2> /dev/null
    if [ -s "$f" ] && [ ! -e "${f%.jsonl}.bindings.json" ] && [ $((t1 - t0)) -lt 2 ]; then
      # a run shorter than the sampler's reach (the dry sizes): not recorded, not failed
      echo "not recorded: the run took less than 2 s" > "${f%.jsonl}.bindings.txt"
    elif [ -s "$f" ] && ! python3 -I "${FIN_HERE}/bindings.py" check "${f%.jsonl}.bindings.json" "$np" "$t" "$cores" > "${f%.jsonl}.bindings.txt"; then
      bad=1; echo "$(basename "$out") $c np$np r$r: $l: core binding not as planned (${f%.jsonl}.bindings.txt)" >> "$D/CHECKS_FAILED"
    fi
  done
  if [ "$rc" != 0 ] && cat "$out"/*/"np$np/$c.r$r.err" 2> /dev/null | grep -q "skip:"; then
    cat "$out"/*/"np$np/$c.r$r.err" | grep -m1 "skip:" >> "$out/.skipped"
    fin_group_record "$D" "$id" skipped "this build or rank count"; fin_progress "$D" "$id: skipped (this build or rank count)"
  elif [ "$rc" = 0 ]; then
    if check_suite_group "$out" "$c" "$r" "np$np" "" "${labels[@]}" && [ "$bad" = 0 ]; then
      fin_group_record "$D" "$id" done; fin_progress "$D" "$id: done"
    else fin_group_record "$D" "$id" check_failed; fin_progress "$D" "$id: CHECK FAILED"; fi
  else
    fin_group_record "$D" "$id" failed; echo "$(fin_utc) $id" >> "$D/FAILED"; fin_progress "$D" "$id: FAILED"
  fi
}

# digests of the bitwise variants equal; threads and precision (the process
# default policy, read from the environment) as requested
check_suite_group() { # outdir case rep npdir grouplog labels...
  local out=$1 c=$2 r=$3 npd=$4 glog=$5; shift 5
  python3 -I - "$out" "$c" "$r" "$npd" "$D/CHECKS_FAILED" "${FIN_HERE}/digest-exceptions.txt" "$glog" "$@" <<'PY'
import json, os, sys
out, case, rep, npd, failed, exc_file, glog = sys.argv[1:8]
labels = sys.argv[8:]
group_log = open(glog, errors="replace").read() if glog and os.path.exists(glog) else ""
exceptions, unrepeatable = set(), set()
if os.path.exists(exc_file):
    for ln in open(exc_file):
        ln = ln.split("#", 1)[0].strip()
        if ln.startswith("!"):
            unrepeatable.add(ln[1:].strip())
        elif ln:
            exceptions.add(ln)
bad, digests = [], {}
for lab in labels:
    f = os.path.join(out, lab, npd, f"{case}.r{rep}.jsonl") if npd else os.path.join(out, lab, f"{case}.r{rep}.jsonl")
    if not os.path.exists(f):
        bad.append(f"{lab}: no result file")
        continue
    want_t = int(lab.rsplit("-t", 1)[1]) if "-t" in lab else 1
    want_p = "fast" if "-fast" in lab else "bitwise"
    lines = [json.loads(x) for x in open(f) if x.startswith("{")]
    if not lines:
        err = f[:-6] + ".err"
        if "skip:" in group_log or (os.path.exists(err) and "skip:" in open(err, errors="replace").read()):
            continue   # the case does not run in this build or at this rank count
        bad.append(f"{lab}: no result line")
        continue
    for o in lines:
        if o.get("threads", 1) != want_t or o.get("precision", "bitwise") != want_p:
            bad.append(f"{lab}: threads {o.get('threads')} precision {o.get('precision', 'absent')}, requested {want_t} {want_p}")
            break
    if want_p == "bitwise":
        digests[lab] = {o.get("digest") for o in lines}
ref = digests.get("head") or digests.get("before")
key = case if not npd else f"{case}@{npd}"
if case in unrepeatable or key in unrepeatable:
    digests = {}   # not repeatable at this rank count: timed only
for lab, d in digests.items():
    if len(d) != 1:
        bad.append(f"{lab}: several digests in one run {sorted(map(str, d))}")
    elif d != ref:
        if lab.startswith("before") and (case in exceptions or key in exceptions):
            continue
        bad.append(f"{lab}: digest {sorted(map(str, d))} != head {sorted(map(str, ref or []))}")
if bad:
    with open(failed, "a") as h:
        for b in bad:
            h.write(f"{os.path.basename(out)} {key} r{rep}: {b}\n")
    print("\n".join(bad), file=sys.stderr)
    sys.exit(1)
PY
}

run_python() { # id rep labels case size
  local id=$1 r=$2 c=$4 sz=$5 labels l rc=0 out="$D/python"
  read -r -a labels <<< "$3"
  for l in "${labels[@]}"; do
    mkdir -p "$out/$l"
    rm -f "$out/$l/$c.r$r.jsonl"
    fin_cmdlog "$D" env OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1 taskset -c "$pin" "$(vdir "$l")/venv/bin/python" -I "$(vdir "$l")/suite_python.py" \
      --case "$c" --size "$sz" --reps 1 --out "$out/$l/$c.r$r.jsonl"
    env OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1 MKL_NUM_THREADS=1 taskset -c "$pin" "$(vdir "$l")/venv/bin/python" -I "$(vdir "$l")/suite_python.py" \
      --case "$c" --size "$sz" --reps 1 --out "$out/$l/$c.r$r.jsonl" > /dev/null 2> "$out/$l/$c.r$r.err" || rc=1
  done
  if [ "$rc" = 0 ]; then
    if check_suite_group "$out" "$c" "$r" "" "" "${labels[@]}"; then fin_group_record "$D" "$id" done; fin_progress "$D" "$id: done"
    else fin_group_record "$D" "$id" check_failed; fin_progress "$D" "$id: CHECK FAILED"; fi
  else
    fin_group_record "$D" "$id" failed; echo "$(fin_utc) $id" >> "$D/FAILED"; fin_progress "$D" "$id: FAILED"
  fi
}

# ----------------------------------------------------------------------------
# Cohesive engine (parts C, D, E, F, G): one run = one c1prep_campaign.sh call

# partitions of the C++ harness (Scotch, AKA_SEED = 1) for mesh H at NP ranks
coh_partition() { # h np
  local h=$1 np=$2 dst="$D/partitions/h$1/partition-np$2.csv" camp
  [ "$np" -gt 1 ] || return 0
  [ -s "$dst" ] && return 0
  mkdir -p "$(dirname "$dst")"
  if [ -s "${FIN_BASELINE}/partitions/h$h/partition-np$np.csv" ]; then
    cp "${FIN_BASELINE}/partitions/h$h/partition-np$np.csv" "$dst"
    fin_progress "$D" "partition h${h} np${np}: from the baseline"
    return 0
  fi
  camp="part-h$h-np$np"
  fin_cmdlog "$D" env OUT="$D/partitions/work" C1PREP_LOCK="$D/c1.lock" bash "$CAMPAIGN_TOOL" --memory "$memory" --export-partition "$camp" "$h" "$np"
  if env OUT="$D/partitions/work" C1PREP_LOCK="$D/c1.lock" bash "$CAMPAIGN_TOOL" --memory "$memory" --export-partition "$camp" "$h" "$np" \
    > "$D/partitions/work-$camp.log" 2>&1; then
    cp "$D/partitions/work/$camp/partition-np$np.csv" "$dst"
    rm -rf "$D/partitions/work/$camp"
    docker run --rm -u 0 -v akantu-perf-cohesive:/perf "${FIN_ORACLE_IMAGE}" rm -rf "/perf/runs/$camp"
    fin_progress "$D" "partition h${h} np${np}: exported"
  else
    fin_progress "$D" "partition h${h} np${np}: FAILED"
    return 1
  fi
}

run_partition() { # id h np
  if coh_partition "$2" "$3"; then fin_group_record "$D" "$1" done
  else fin_group_record "$D" "$1" failed; echo "$(fin_utc) $1" >> "$D/FAILED"; fi
}

# one run; prints the outcome (ok | ud216 | oom | failed)
coh_run() { # group series h steps np rep variant attempt placement
  local group=$1 series=$2 h=$3 steps=$4 np=$5 rep=$6 v=$7 att=$8 placement=$9 c b t p opts=() extra=() camp outcome cores sha
  t=$(threads_of "$v"); p=$(precision_of "$v")
  case "$v" in
    cpp) c=cpp; sha=$(cpp_sha cohesive_extrinsic_strict) ;;
    cppeig) c=cppeig; sha=$(cpp_sha cohesive_extrinsic_strict) ;;
    *) b=${v%%-*}; c=rust
       case "$v" in *-eig*) c=rusteig ;; esac
       case "$v" in *-release*) extra+=(--rust-extra "rust_release_static_solver=true") ;; esac
       sha=${RSHA[$b]}
       opts+=(--rust-bin "${RBIN[$b]}" --rust-volume "${FIN_TVOL}" --rust-image "${FIN_MPI_IMAGE}")
       [ "$t" = 1 ] || opts+=(--rust-threads "$t")
       [ "$p" = bitwise ] || opts+=(--rust-precision "$p") ;;
  esac
  camp="${series}-h${h}-np${np}-r${rep}-${v}"
  [ "$att" = 1 ] || camp="${camp}-a${att}"
  rm -rf "${D:?}/runs/${camp}"
  cores=$(fin_run_cores "$np" "$t" "$placement" "$CORES_ORDER")
  local partopt=()
  [ "$np" = 1 ] || partopt=(--partitions "$D/partitions/h$h")
  local cmd=(env OUT="$D/runs" PIN_CPUS_LIST="$cores" C1PREP_LOCK="$D/c1.lock" FNTE=joss CPP_CONFIG=strict C1_CONTAINER_PREFIX=final PERF_MPI_BIND=core
    bash "$CAMPAIGN_TOOL" --memory "$memory" --cpus "$NCORES" ${partopt[@]+"${partopt[@]}"} ${opts[@]+"${opts[@]}"}
    ${extra[@]+"${extra[@]}"} "$camp" "$h" "$steps" "$np" 1 "$c")
  fin_cmdlog "$D" "${cmd[@]}"
  local t0 t1 rec
  rm -f "$D/runs/${camp}.bindings.json"
  python3 -I "${FIN_HERE}/bindings.py" record "$D/runs/${camp}.bindings.json" "$np" > /dev/null 2>&1 &
  rec=$!
  t0=$(date +%s)
  "${cmd[@]}" > "$D/runs/${camp}.log" 2>&1
  t1=$(date +%s)
  kill "$rec" 2> /dev/null; wait "$rec" 2> /dev/null
  outcome=$(sed -n 's/^outcome="\(.*\)"$/\1/p' "$D/runs/$camp/$c/np$np/rep1/status.txt" 2> /dev/null)
  [ -n "$outcome" ] || outcome=failed
  docker run --rm -u 0 -v akantu-perf-cohesive:/perf -v "${FIN_TVOL}:/target" "${FIN_ORACLE_IMAGE}" \
    rm -rf "/perf/runs/$camp" "/target/c1prep/$camp" > /dev/null 2>&1
  python3 -I -c 'import json,sys; k=["group","series","h","steps","np","rep","variant","case","threads","precision","attempt","campaign","outcome","utc_start","wall_s","placement","cores","binary_sha256","session"]
v=sys.argv[1:]; d=dict(zip(k,v)); [d.__setitem__(x,int(d[x])) for x in ("steps","np","rep","threads","attempt")]; d["wall_s"]=float(d["wall_s"])
print(json.dumps(d))' "$group" "$series" "$h" "$steps" "$np" "$rep" "$v" "$c" "$t" "$p" "$att" "$camp" "$outcome" \
    "$(date -u -d "@$t0" +%FT%TZ)" "$((t1 - t0))" "$placement" "$cores" "$sha" "$FIN_SESSION" >> "$D/index.jsonl"
  echo "$outcome"
}

# a group: the variants of one point in one repetition, in the plan's order;
# repeated (at most twice) when a C++ run took upstream's MUMPS retry path
# (UD-216); then the checks
run_coh() { # id rep labels series h steps np placement
  local id=$1 rep=$2 series=$4 h=$5 steps=$6 np=$7 placement=$8 order v att=1 res ud allok
  read -r -a order <<< "$3"
  mkdir -p "$D/runs"
  coh_partition "$h" "$np" || { fin_group_record "$D" "$id" failed "no partition"; echo "$(fin_utc) $id: no partition" >> "$D/FAILED"; return 0; }
  while :; do
    ud=0 allok=1
    for v in "${order[@]}"; do
      res=$(coh_run "$id" "$series" "$h" "$steps" "$np" "$rep" "$v" "$att" "$placement")
      [ "$res" = ok ] || allok=0
      [ "$res" = ud216 ] && ud=1
      fin_progress "$D" "${series} h${h} np${np} r${rep} ${v}: ${res}$([ "$att" = 1 ] || echo " (attempt ${att})")"
    done
    if [ "$ud" = 1 ] && [ "$att" -lt 3 ]; then att=$((att + 1)); continue; fi
    break
  done
  if [ "$allok" = 1 ]; then
    if coh_check "$series" "$h" "$np" "$rep" "$att" "$placement" "${order[@]}"; then fin_group_record "$D" "$id" done
    else fin_group_record "$D" "$id" check_failed; fi
  else
    fin_group_record "$D" "$id" failed; echo "$(fin_utc) $id" >> "$D/FAILED"
  fi
}

# per run: the binary that ran is the one of VARIANT.json / CPP.json; the core
# binding is the planned one; the thread count and precision of each port run
# (rust_exec: the process default policy read from the environment) are the
# requested ones; at one rank (and in the SparseLU series at every rank count)
# the per-step energies and the insertions of every bitwise variant exist and
# are identical (and equal to C++ at one rank)
coh_check() { # series h np rep attempt placement variants...
  local series=$1 h=$2 np=$3 rep=$4 att=$5 placement=$6; shift 6
  local exp=() v
  for v in "$@"; do
    case "$v" in cpp|cppeig) exp+=("$v=$(cpp_sha cohesive_extrinsic_strict):$(fin_run_cores "$np" 1 "$placement" "$CORES_ORDER")") ;;
      *) exp+=("$v=${RSHA[${v%%-*}]}:$(fin_run_cores "$np" "$(threads_of "$v")" "$placement" "$CORES_ORDER")") ;;
    esac
  done
  python3 -I - "$D" "$series" "$h" "$np" "$rep" "$att" "${FIN_HERE}/bindings.py" "${exp[@]}" <<'PY'
import json, os, subprocess, sys
D, series, h, np, rep, att, bindings = sys.argv[1:8]
expect = dict(x.split("=", 1) for x in sys.argv[8:])
variants = list(expect)
bad, files = [], {}
for v in variants:
    want_sha, cores = expect[v].split(":", 1)
    camp = f"{series}-h{h}-np{np}-r{rep}-{v}" + ("" if att == "1" else f"-a{att}")
    threads = int(v.rsplit("-t", 1)[1]) if "-t" in v else 1
    bf = os.path.join(D, "runs", camp + ".bindings.json")
    chk = subprocess.run([sys.executable, "-I", bindings, "check", bf, np, str(threads), cores],
                         capture_output=True, text=True)
    with open(os.path.join(D, "runs", camp + ".bindings.txt"), "w") as h_:
        h_.write(chk.stdout)
    if chk.returncode != 0:
        bad.append(f"{v}: core binding not as planned ({camp}.bindings.txt): "
                   + "; ".join(ln for ln in chk.stdout.splitlines() if ln.startswith("BINDING"))[:300])
    case = "cpp" if v == "cpp" else "cppeig" if v == "cppeig" else ("rusteig" if "-eig" in v else "rust")
    d = os.path.join(D, "runs", camp, case, f"np{np}", "rep1")
    if not os.path.isdir(d):
        bad.append(f"{v}: no run directory")
        continue
    if case.startswith("cpp"):
        try:
            got = json.load(open(os.path.join(d, "run.json"))).get("binary_sha256", "")
        except (OSError, ValueError):
            got = ""
    else:
        try:
            got = open(os.path.join(d, "binary.sha256")).read().split()[0]
        except (OSError, IndexError):
            got = ""
    if got != want_sha:
        bad.append(f"{v}: the binary that ran ({got[:16] or 'unknown'}) is not the planned one ({want_sha[:16]})")
    if not case.startswith("cpp"):
        want_t = threads
        want_p = "fast" if "-fast" in v else "bitwise"
        try:
            s = json.load(open(os.path.join(d, "summary.json")))
        except (OSError, ValueError):
            s = {}
            bad.append(f"{v}: no summary.json")
        ex = s.get("rust_exec", {"threads": 1, "precision": "bitwise", "absent": True})
        if ex.get("threads") != want_t or ex.get("precision") != want_p:
            bad.append(f"{v}: rust_exec {ex}, requested threads {want_t} precision {want_p}")
        if want_p == "fast":
            continue
    e = os.path.join(d, "energies.csv")
    i = os.path.join(d, "insertions.csv")
    if not os.path.exists(e) or not os.path.exists(i):
        bad.append(f"{v}: energies.csv or insertions.csv missing")
        continue
    files[v] = (open(e, "rb").read(), open(i, "rb").read())
# which runs must be identical: the port's bitwise variants with the same solver;
# with C++ at one rank (the same MUMPS or SparseLU on the same matrix)
groups = {}
for v, data in files.items():
    solver = "eig" if ("eig" in v) else "mumps"
    if v.startswith("cpp") and np != "1":
        continue
    if not v.startswith("cpp") and np != "1" and solver == "mumps":
        continue   # the distributed MUMPS solve does not repeat run to run
    groups.setdefault(solver, []).append((v, data))
for solver, runs in groups.items():
    ref_v, ref = runs[0]
    for v, data in runs[1:]:
        if data[0] != ref[0]:
            bad.append(f"energies.csv of {v} differ from {ref_v} ({solver})")
        elif data[1] != ref[1]:
            bad.append(f"insertions.csv of {v} differ from {ref_v} ({solver})")
with open(os.path.join(D, "checks.jsonl"), "a") as h_:
    h_.write(json.dumps({"series": series, "h": h, "np": int(np), "rep": int(rep),
                         "compared": {s: [v for v, _ in r] for s, r in groups.items()}, "failures": bad}) + "\n")
if bad:
    with open(os.path.join(D, "CHECKS_FAILED"), "a") as h_:
        for b in bad:
            h_.write(f"{series} h{h} np{np} r{rep}: {b}\n")
    print("\n".join(bad), file=sys.stderr)
    sys.exit(1)
PY
}

# ----------------------------------------------------------------------------
# Part F: the MPI layer against raw MPI (docs/benchmarks/mpi-latency-2026-11-14)
run_in_mpi() { # np prog-dir prog args... (akantu-rs-mpi:dev, the first np cores of the NUMA-ordered list)
  local n=$1 pd=$2; shift 2
  docker run --rm --init --name "final-mpi-$$-$RANDOM" --cpuset-cpus "$(fin_run_cores "$n" 1 fill "$CORES_ORDER")" \
    --shm-size 2g -v "$pd:/b:ro" akantu-rs-mpi:dev timeout -k 5 300 mpirun --allow-run-as-root --map-by core --bind-to core "$@"
}

# method A: separate launches of mpi_bench (before, head) and bench_baseline (raw)
run_mpi_a() { # id rep labels btl np kind bytes iters
  local id=$1 r=$2 btl=$4 np=$5 kind=$6 bytes=$7 it=$8 labels l line us rows=() ok=1 out="$D/mpilayer"
  read -r -a labels <<< "$3"
  mkdir -p "$out"
  [ -s "$out/separate.tsv" ] || printf 'rep\tbtl\tnp\tkind\tbytes\tprog\tus\tgroup\n' > "$out/separate.tsv"
  for l in "${labels[@]}"; do
    if [ "$l" = raw ]; then
      line=$(run_in_mpi "$np" "${FIN_BIN}/raw" --mca btl "$btl,self" -np "$np" /b/bench_baseline "$kind" "$bytes" "$it" 2>> "$out/err.log" | grep -a '^BASE' | head -1)
      us=$(printf '%s\n' "$line" | sed -n 's/.*half-RTT \([0-9.]*\) us.*/\1/p; s/.*: \([0-9.]*\) us per exchange.*/\1/p')
    else
      line=$(run_in_mpi "$np" "$(vdir "$l")/mpi" --mca btl "$btl,self" -np "$np" /b/mpi_bench "$kind" "$bytes" "$it" 2>> "$out/err.log" | grep -a '^PERF' | head -1)
      echo "$id $l $line" >> "$out/perf-lines.txt"
      us=$(printf '%s\n' "$line" | grep -o '[0-9.]* us' | head -1 | cut -d' ' -f1)
    fi
    [ -n "$us" ] || ok=0
    rows+=("$(printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s' "$r" "$btl" "$np" "$kind" "$bytes" "$l" "${us:-NA}" "$id")")
  done
  printf '%s\n' "${rows[@]}" >> "$out/separate.tsv"
  if [ "$ok" = 1 ]; then fin_group_record "$D" "$id" done; fin_progress "$D" "$id: done"
  else fin_group_record "$D" "$id" failed; echo "$(fin_utc) $id" >> "$D/FAILED"; fin_progress "$D" "$id: FAILED (err.log)"; fi
}

# method B: `mpi_bench calibrated KIND BYTES ITERATIONS 9`, one job per
# variant (raw MPI and akantu-comm interleaved inside it); one group = the pair
run_mpi_b() { # id rep labels btl np kind bytes iters
  local id=$1 r=$2 btl=$4 np=$5 kind=$6 bytes=$7 it=$8 labels l line ok=1 out="$D/mpilayer" lines=()
  read -r -a labels <<< "$3"
  mkdir -p "$out"
  for l in "${labels[@]}"; do
    line=$(run_in_mpi "$np" "$(vdir "$l")/mpi" --mca btl "$btl,self" -np "$np" /b/mpi_bench calibrated "$kind" "$bytes" "$it" 9 \
      2>> "$out/err.log" | grep -a '^CAL summary' | head -1)
    [ -n "$line" ] || ok=0
    lines+=("rep=$r btl=$btl np=$np prog=$l group=$id $line")
  done
  printf '%s\n' "${lines[@]}" >> "$out/calibrated.txt"
  if [ "$ok" = 1 ]; then fin_group_record "$D" "$id" done; fin_progress "$D" "$id: done"
  else fin_group_record "$D" "$id" failed; echo "$(fin_utc) $id" >> "$D/FAILED"; fin_progress "$D" "$id: FAILED (err.log)"; fi
}

# ----------------------------------------------------------------------------
# the executor: every group of the plan, in order
if [ "$part" = G ] && grep -q $'\tjoss\t' "$D/plan.tsv"; then
  docker run --rm -v akantu-perf-cohesive:/perf:ro "${FIN_ORACLE_IMAGE}" test -f /perf/meshes/cube-joss.msh \
    || fin_die "part G: /perf/meshes/cube-joss.msh missing (prepare.sh --joss-mesh FILE)"
fi
stopped=0
while IFS=$'\t' read -r -u 3 kind id rep vars a b c d e; do
  [ -e "$D/.done/$id" ] && continue
  if ! why=$(fin_wait_idle); then
    fin_group_record "$D" "$id" failed "machine not idle: $why"
    echo "$(fin_utc) $id: machine not idle ($why); the part stops" >> "$D/FAILED"
    fin_progress "$D" "the machine is not idle ($why): the part stops"
    stopped=1; break
  fi
  case "$kind" in
    a_digests) run_a_digests "$id" "$vars" "$a" ;;
    a_round) run_a_round "$id" "$rep" "$vars" "$a" ;;
    suite) run_suite "$id" "$rep" "$vars" "$a" "$b" "$c" "$d" ;;
    suite_mpi) run_suite_mpi "$id" "$rep" "$vars" "$a" "$b" "$c" "$d" "$e" ;;
    python) run_python "$id" "$rep" "$vars" "$a" "$b" ;;
    partition) run_partition "$id" "$a" "$b" ;;
    coh) run_coh "$id" "$rep" "$vars" "$a" "$b" "$c" "$d" "$e" ;;
    mpi_a) run_mpi_a "$id" "$rep" "$vars" "$a" "$b" "$c" "$d" "$e" ;;
    mpi_b) run_mpi_b "$id" "$rep" "$vars" "$a" "$b" "$c" "$d" "$e" ;;
    *) fin_die "plan: unknown group kind $kind" ;;
  esac
done 3< "$D/plan.tsv"

fin_scrub_tree "$D"
status=0
if [ -s "$D/FAILED" ]; then fin_progress "$D" "part ${part}: failed runs (FAILED)"; status=1; fi
if [ -s "$D/CHECKS_FAILED" ]; then fin_progress "$D" "part ${part}: failed checks (CHECKS_FAILED)"; status=1; fi
[ "$stopped" = 0 ] || status=1
if ! fin_guard_tree "$D" > "$D/.guard" 2>&1; then fin_progress "$D" "part ${part}: the data names the machine (.guard)"; status=1; fi
python3 -I "${FIN_HERE}/summarize.py" "$D" > "$D/.summarize.log" 2>&1 || { fin_progress "$D" "part ${part}: summarize.py failed (.summarize.log)"; status=1; }
fin_progress "$D" "part ${part} end: status ${status}"
exit "$status"
