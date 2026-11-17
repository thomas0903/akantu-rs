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
#                 suite_python.py): serial cases, threads, fast mode, the rank
#                 series of the distributed cases
#              C  the cohesive benchmark at small and mid sizes, 1 to 8 ranks
#              D  static solves: SparseLU against upstream's, MUMPS at 1/2/4
#                 ranks, the solver phases of the suite
#              E  memory: peak resident set per rank and summed, with the
#                 option that releases the static solver's factors
#              F  set-up phases, the distribution at size l, the MPI layer
#              G  the full-size run (JOSS mesh; one 64-core node)
#   --reps N      repetitions (default 5; part G: 1)
#   --size S      dry (the smallest sizes: a development check of the whole
#                 chain, not a result) or full (the sizes of the published
#                 tables; default)
#   --only LIST   comma list of the part's series (default all; to split a
#                 part over machines, see README.md)
#   --root DIR    FINAL_ROOT (default ~/final): binaries in DIR/bin, data in DIR/data
#   --data DIR    data root (default FINAL_ROOT/data); the part writes DIR/<PART>
#   --before DIR, --head DIR   prepared variant directories (default
#                 FINAL_ROOT/bin/before, FINAL_ROOT/bin/head; prepare.sh)
#   --pin CORE    the core of the serial runs (default 2); N threads or ranks
#                 run on cores 0..N-1
#   --memory M    container memory limit of the cohesive runs, no swap
#                 (default: the machine's memory minus 3 GiB)
#   --points "P..."  part G only: the points in their order, P = RANKS[xTHREADS]:VARIANT[,VARIANT]
#                 (default "64:head,cpp 32:head,cpp 32x2:head 8:head"; dry
#                 "8:head,cpp 4x2:head")
#
# Each point (a group of runs: the variants of one configuration in one
# repetition, their order rotated by the repetition) writes its own
# directory and a marker when it is complete; a second call skips complete
# points and repeats incomplete ones. One progress line per point
# (DATA/<PART>/progress.log). Checks (state digests of every bitwise
# variant, thread count and precision recorded by every run, upstream's
# MUMPS retry path) that fail are written to DATA/<PART>/CHECKS_FAILED; a
# failed run to DATA/<PART>/FAILED. Exit status: 0 if every point is
# complete and every check passed, 1 otherwise, 2 for a usage error. Safe
# under nohup (no terminal needed). Run one part at a time on a machine,
# nothing else running.
set -uo pipefail
# shellcheck source=lib.sh
. "$(dirname "$0")/lib.sh"

part=${1:-}
[ -n "$part" ] || { sed -n '6,47p' "$0"; exit 2; }
shift
reps="" size=full only="" data="" pin=2 memory="" before="" head="" gpoints=""
while [ $# -gt 0 ]; do
  case "$1" in
    --reps) reps=${2:?}; shift 2 ;;
    --size) size=${2:?}; shift 2 ;;
    --only) only=${2:?}; shift 2 ;;
    --root) FINAL_ROOT=${2:?}; shift 2 ;;
    --data) data=${2:?}; shift 2 ;;
    --before) before=${2:?}; shift 2 ;;
    --head) head=${2:?}; shift 2 ;;
    --pin) pin=${2:?}; shift 2 ;;
    --memory) memory=${2:?}; shift 2 ;;
    --points) gpoints=${2:?}; shift 2 ;;
    *) fin_die "run_part.sh: unknown argument $1" ;;
  esac
done
case "$part" in A|B|C|D|E|F|G) ;; *) fin_die "run_part.sh: unknown part ${part}" ;; esac
case "$size" in dry|full) ;; *) fin_die "--size dry|full" ;; esac
[ -n "$reps" ] || { if [ "$part" = G ]; then reps=1; else reps=5; fi; }
[[ "$reps" =~ ^[1-9][0-9]*$ ]] || fin_die "--reps: a positive integer"
FIN_BIN="${FINAL_ROOT}/bin"
BEFORE="${before:-${FIN_BIN}/before}"
HEAD="${head:-${FIN_BIN}/head}"
[ -f "${BEFORE}/VARIANT.json" ] || fin_die "no prepared variant ${BEFORE} (prepare.sh)"
[ -f "${HEAD}/VARIANT.json" ] || fin_die "no prepared variant ${HEAD} (prepare.sh)"
D="${data:-${FINAL_ROOT}/data}/${part}"
mkdir -p "$D"
D="$(cd "$D" && pwd)"
NCORES=$(lscpu -p=CORE 2> /dev/null | grep -v '^#' | sort -u | wc -l)
[ "$NCORES" -ge 1 ] || NCORES=$(nproc)
[ -n "$memory" ] || memory="$(( $(awk '/MemTotal/ {print $2}' /proc/meminfo) / 1048576 - 3 ))g"
CAMPAIGN_TOOL="${FIN_REPO}/crates/akantu-bench/scripts/c1prep_campaign.sh"
PYSUITE="${FIN_REPO}/python/benchmarks/suite_python.py"
X="${FIN_REPO}/scripts/perf/explicit-step"
exec 8> "${D}/.lock"
flock -n 8 || fin_die "another run_part.sh holds ${D}/.lock"

# failures of an earlier session are kept as history: this session repeats
# every incomplete point and reports its own outcome
for f in FAILED CHECKS_FAILED; do
  if [ -s "$D/$f" ]; then mkdir -p "$D/history"; mv "$D/$f" "$D/history/$f.$(date -u +%Y%m%dT%H%M%SZ)"; fi
done
fin_provenance "$D" "$part"
fin_cmdlog "$D" "$0" "$part" --reps "$reps" --size "$size" ${only:+--only "$only"} --pin "$pin" --memory "$memory"
{
  echo "part=${part} size=${size} reps=${reps} only=${only:-all} pin=${pin} memory=${memory} cores=${NCORES}"
  echo "before=$(fin_variant_key "$BEFORE" revision) head=$(fin_variant_key "$HEAD" revision) cpp=${FIN_CPP_REF}"
} | fin_scrub >> "$D/provenance.txt"
want_series() { [ -z "$only" ] && return 0; case ",${only}," in *",$1,"*) return 0 ;; *) return 1 ;; esac; }
fin_progress "$D" "part ${part} start: size ${size}, ${reps} repetitions${only:+, series ${only}}"

# variant label -> host directory of its base variant
vdir() { case "${1%%-*}" in before) echo "$BEFORE" ;; head) echo "$HEAD" ;; *) fin_die "variant $1" ;; esac; }
# labels of the timing parts that need more cores than the machine has are dropped
fits() { local t; read -r _ t _ <<< "$(fin_parse_label "$1")"; [ "$t" -le "$NCORES" ]; }

# ----------------------------------------------------------------------------
# Part A: the explicit step
part_a() {
  local progs="$D/progs" labels=() l b t p cfgs mesh mode n fd rec rp rs rdd ok r
  local all="cpp-strict cpp-default before head head-t2 head-t4 head-t8 head-fast head-fast-t8"
  for l in $all; do
    case "$l" in cpp-*) labels+=("$l"); continue ;; esac
    fits "$l" || { fin_progress "$D" "A: ${l} needs more cores than ${NCORES}: dropped"; continue; }
    read -r b t p <<< "$(fin_parse_label "$l")"
    if [ "$t" = 1 ]; then fin_wrapper "$progs" "$l" "$(vdir "$l")/explicit_step_timing" 1 "$p" "$pin"
    else fin_wrapper "$progs" "$l" "$(vdir "$l")/explicit_step_timing" "$t" "$p" 0; fi
    labels+=("$l")
  done
  if [ "$size" = full ]; then
    cfgs="tet4-20k:30 tet4-20k:30:fd tet4-200k:6 tet4-200k:6:fd tet10-20k:6 tet10-20k:6:fd hex8-20k:8 hex8-20k:8:fd hex8-200k:3 hex8-200k:3:fd"
  else
    cfgs="tet4-20k:3 tet4-20k:3:fd tet10-20k:2 tet10-20k:2:fd hex8-20k:2 hex8-20k:2:fd"
  fi
  local meshes="${FINAL_ROOT}/meshes/explicit"
  rec="${FIN_REPO}/docs/benchmarks/explicit-step/final/digests.txt"
  # 1. state digests (once): every bitwise program equal, and equal to the
  #    recorded digests (port = C++ strict; C++ default its own)
  if want_series digests && [ ! -e "$D/digests.ok" ]; then
    fin_progress "$D" "A digests: start"
    : > "$D/digests.tsv"
    ok=1
    for c in $cfgs; do
      IFS=: read -r mesh _ fd <<< "$c"; mode=ss; [ "${fd:-}" = fd ] && mode=fd
      n=$(awk -v m="$mesh" -v o="$mode" '$1 == m && $2 == o {sub("n=", "", $3); print $3}' "$rec")
      read -r rp rs rdd <<< "$(awk -v m="$mesh" -v o="$mode" '$1 == m && $2 == o {print $4, $7, $8}' "$rec")"
      local ref="" dig
      for l in "${labels[@]}"; do
        case "$l" in
          cpp-*) dig=$(PIN_CPU="$pin" "$X/cpp_run.sh" "${FIN_BIN}/cpp" "${l#cpp-}" "$meshes" "$mesh" 1 1 0 \
                   "$([ "$mode" = fd ] && echo 1 || echo 0)" check "$n" | sed -n 's/.*"fnv1a":"\([0-9a-f]*\)".*/\1/p') ;;
          *) local fdarg=(); [ "$mode" = fd ] && fdarg=(--fd)
             dig=$("$progs/$l" --mesh "$meshes/$mesh.msh" --digest-steps "$n" "${fdarg[@]}" | sed -n 's/.*"fnv1a":"\([0-9a-f]*\)".*/\1/p') ;;
        esac
        local verdict
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
    if [ "$ok" = 1 ]; then touch "$D/digests.ok"; fin_progress "$D" "A digests: all equal (digests.tsv)"
    else fin_check_fail "$D" "A digests: a bitwise program differs (digests.tsv)"; fi
  fi
  # 2. rounds: every configuration with every program, order rotated per round
  want_series rounds || return 0
  mkdir -p "$D/rounds"
  for ((r = 1; r <= reps; r++)); do
    [ -e "$D/rounds/r$r.ok" ] && { fin_progress "$D" "A round ${r}: complete, skipped"; continue; }
    rm -f "$D/rounds/r$r.jsonl"
    fin_cmdlog "$D" env ROUND_START="$r" PIN_CPU="$pin" BIN_DIR="$progs" CPP_BIN_DIR="${FIN_BIN}/cpp" MESH_DIR="$meshes" \
      "$X/timing_rotated.sh" "$D/rounds/r$r.jsonl" 1 "${labels[*]}" "$cfgs"
    if env ROUND_START="$r" PIN_CPU="$pin" BIN_DIR="$progs" CPP_BIN_DIR="${FIN_BIN}/cpp" MESH_DIR="$meshes" \
      "$X/timing_rotated.sh" "$D/rounds/r$r.jsonl" 1 "${labels[*]}" "$cfgs" > "$D/rounds/r$r.log" 2>&1 \
      && check_a_round "$D/rounds/r$r.jsonl" "${labels[@]}"; then
      touch "$D/rounds/r$r.ok"; fin_progress "$D" "A round ${r}/${reps}: done"
    else
      echo "$(fin_utc) A round ${r}" >> "$D/FAILED"; fin_progress "$D" "A round ${r}: FAILED (rounds/r$r.log)"
    fi
  done
}

# every program's time lines present and every port meta line with the requested threads and precision
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
        seen.setdefault(o["label"].split("|")[0], 0)
        seen[o["label"].split("|")[0]] += 1
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
# Suites (parts B, D, F): serial groups with run_ab.sh, MPI groups in the image

# serial group: one case, one repetition, the variants rotated (run_ab.sh)
suite_serial_group() { # outdir bin(suite_solid...) case sz rep labels...
  local out=$1 bin=$2 c=$3 sz=$4 r=$5; shift 5
  local mark="$out/.done/$c.r$r" args=() l lst
  [ -e "$mark" ] && return 0
  mkdir -p "$out/.done" "$out/progs"
  for l in "$@"; do
    local b t p; read -r b t p <<< "$(fin_parse_label "$l")"
    lst="$out/.list-${b}"
    [ -s "$lst" ] || "$(vdir "$l")/$bin" --list 2> /dev/null | awk '{print $1}' > "$lst"
    grep -qx "$c" "$lst" || continue   # the case does not exist in that variant
    if [ "$t" = 1 ]; then fin_wrapper "$out/progs" "$l" "$(vdir "$l")/$bin" 1 "$p" "$pin"
    else fin_wrapper "$out/progs" "$l" "$(vdir "$l")/$bin" "$t" "$p" 0; fi
    args+=("$l=$out/progs/$l")
  done
  [ ${#args[@]} -gt 0 ] || return 0
  # rotate the variants by the repetition (run_ab.sh rotates by its offset too)
  fin_cmdlog "$D" "${FIN_REPO}/scripts/perf/suite/run_ab.sh" --out "$out" --size "$sz" --reps 1 --rep-offset "$r" \
    --cases "$c" --core "$pin" "${args[@]}"
  if "${FIN_REPO}/scripts/perf/suite/run_ab.sh" --out "$out" --size "$sz" --reps 1 --rep-offset "$r" \
    --cases "$c" --core "$pin" "${args[@]}" > "$out/.log.$c.r$r" 2>&1; then
    check_suite_group "$out" "$c" "$r" "" "$out/.log.$c.r$r" "${args[@]%%=*}" && touch "$mark"
    fin_progress "$D" "$(basename "$out") ${c} r${r}: done"
  else
    echo "$(fin_utc) $(basename "$out") $c r$r" >> "$D/FAILED"
    fin_progress "$D" "$(basename "$out") ${c} r${r}: FAILED ($out/.log.$c.r$r)"
  fi
}

# MPI group: one case at P ranks (x T threads), one repetition, variants rotated
suite_mpi_group() { # outdir bin case sz np rep labels...
  local out=$1 bin=$2 c=$3 sz=$4 np=$5 r=$6; shift 6
  local mark="$out/.done/$c.np$np.r$r" order l rc=0 names=()
  [ -e "$mark" ] && return 0
  mkdir -p "$out/.done"
  read -r -a order <<< "$(fin_rotate "$r" "$@")"
  for l in "${order[@]}"; do
    local b t p; read -r b t p <<< "$(fin_parse_label "$l")"
    [ $((np * t)) -le "$NCORES" ] || continue
    local f="$out/$l/np$np/$c.r$r.jsonl" envs=() mapping=(--map-by core --bind-to core)
    mkdir -p "$(dirname "$f")"
    [ "$t" = 1 ] || { envs+=(-e "AKANTU_NUM_THREADS=$t"); mapping=(--map-by "slot:PE=$t" --bind-to core -x AKANTU_NUM_THREADS); }
    [ "$p" = bitwise ] || { envs+=(-e "AKANTU_PRECISION=$p"); mapping+=(-x AKANTU_PRECISION); }
    local cmd=(docker run --rm --init --cpuset-cpus "0-$((np * t - 1))" --shm-size 2g -e SCOTCH_PTHREAD_NUMBER=1
      -e OMP_NUM_THREADS=1 -e OPENBLAS_NUM_THREADS=1 ${envs[@]+"${envs[@]}"} -v "$(vdir "$l")/mpi:/bench:ro" "${FIN_MPI_IMAGE}"
      mpirun --allow-run-as-root -n "$np" "${mapping[@]}" -x SCOTCH_PTHREAD_NUMBER -x OMP_NUM_THREADS -x OPENBLAS_NUM_THREADS
      "/bench/$bin" --case "$c" --size "$sz" --reps 1)
    fin_cmdlog "$D" "${cmd[@]}"
    "${cmd[@]}" > "$f" 2> "${f%.jsonl}.err" || rc=1
    names+=("$l")
  done
  if [ "$rc" = 0 ]; then
    check_suite_group "$out" "$c" "$r" "np$np" "" "${names[@]}" && touch "$mark"
    fin_progress "$D" "$(basename "$out") ${c} np${np} r${r}: done"
  else
    echo "$(fin_utc) $(basename "$out") $c np$np r$r" >> "$D/FAILED"
    fin_progress "$D" "$(basename "$out") ${c} np${np} r${r}: FAILED"
  fi
}

# digests of the bitwise variants equal; threads and precision as requested
check_suite_group() { # outdir case rep npdir grouplog labels...
  local out=$1 c=$2 r=$3 npd=$4 glog=$5; shift 5
  python3 -I - "$out" "$c" "$r" "$npd" "$D/CHECKS_FAILED" "${FIN_HERE}/digest-exceptions.txt" "$glog" "$@" <<'PY'
import json, os, sys
out, case, rep, npd, failed, exc_file, glog = sys.argv[1:8]
labels = sys.argv[8:]
group_log = open(glog, errors="replace").read() if glog and os.path.exists(glog) else ""
exceptions = set()
if os.path.exists(exc_file):
    for ln in open(exc_file):
        ln = ln.split("#", 1)[0].strip()
        if ln:
            exceptions.add(ln)
bad, digests = [], {}
for lab in labels:
    f = os.path.join(out, lab, npd, f"{case}.r{rep}.jsonl") if npd else os.path.join(out, lab, f"{case}.r{rep}.jsonl")
    if not os.path.exists(f):
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

# the case list of a suite binary (head; the cases before lacks are run without before)
suite_cases() { "${HEAD}/$1" --list 2> /dev/null | awk '{print $1}'; }

# ----------------------------------------------------------------------------
# Part B: the suites
SERIAL_LABELS="before head head-t2 head-t4 head-t8 head-fast head-fast-t8"
part_b() {
  local sz=m s c r cases labels=() l np
  [ "$size" = dry ] && sz=s
  for l in $SERIAL_LABELS; do fits "$l" && labels+=("$l"); done
  for s in solid models setup; do
    want_series "$s" || continue
    mapfile -t cases < <(suite_cases "suite_$s")
    for ((r = 0; r < reps; r++)); do
      read -r -a cases_r <<< "$(fin_rotate "$r" "${cases[@]}")"
      for c in "${cases_r[@]}"; do suite_serial_group "$D/suite-$s" "suite_$s" "$c" "$sz" "$r" "${labels[@]}"; done
    done
  done
  # the distributed cases at 1 / 2 / 4 / 8 ranks; ranks x threads for head
  if want_series mpi; then
    local spec bin mcases
    for spec in "suite_solid:par_explicit_tet4_ss,par_cohesive_extrinsic" \
      "suite_setup:explicit_step_dist_tet4,dist_mesh_tet4,dist_facets_tet4,ghost_nodes_tet4,ghost_elements_tet4,reduce_scalars" \
      "suite_models:heat_explicit_tet4,phasefield_staggered_tri3"; do
      bin=${spec%%:*}; IFS=, read -r -a mcases <<< "${spec#*:}"
      for ((r = 0; r < reps; r++)); do
        for c in "${mcases[@]}"; do
          for np in 1 2 4 8; do
            [ "$np" -le "$NCORES" ] || continue
            local vs=(before head)
            [ "$np" = 2 ] && vs+=(head-t4)
            [ "$np" = 4 ] && vs+=(head-t2)
            suite_mpi_group "$D/mpi-${bin#suite_}" "$bin" "$c" "$sz" "$np" "$r" "${vs[@]}"
          done
        done
      done
    done
  fi
  # the Python suite: the same script with each variant's extension
  if want_series python; then
    local out="$D/python" pcases
    mkdir -p "$out/.done"
    mapfile -t pcases < <("${HEAD}/venv/bin/python" -I "$PYSUITE" --list 2> /dev/null | awk '{print $1}')
    for ((r = 0; r < reps; r++)); do
      for c in "${pcases[@]}"; do
        [ -e "$out/.done/$c.r$r" ] && continue
        local rc=0 order
        read -r -a order <<< "$(fin_rotate "$r" before head)"
        for l in "${order[@]}"; do
          mkdir -p "$out/$l"
          fin_cmdlog "$D" env OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1 taskset -c "$pin" "$(vdir "$l")/venv/bin/python" -I "$PYSUITE" \
            --case "$c" --size "$sz" --reps 1 --out "$out/$l/$c.r$r.jsonl"
          rm -f "$out/$l/$c.r$r.jsonl"
          env OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1 MKL_NUM_THREADS=1 taskset -c "$pin" "$(vdir "$l")/venv/bin/python" -I "$PYSUITE" \
            --case "$c" --size "$sz" --reps 1 --out "$out/$l/$c.r$r.jsonl" > /dev/null 2> "$out/$l/$c.r$r.err" || rc=1
        done
        if [ "$rc" = 0 ]; then
          check_suite_group "$out" "$c" "$r" "" "" before head && touch "$out/.done/$c.r$r"
          fin_progress "$D" "python ${c} r${r}: done"
        else
          echo "$(fin_utc) python $c r$r" >> "$D/FAILED"; fin_progress "$D" "python ${c} r${r}: FAILED"
        fi
      done
    done
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
    echo "$(fin_utc) partition h$h np$np" >> "$D/FAILED"; fin_progress "$D" "partition h${h} np${np}: FAILED"
    return 1
  fi
}

# one run; prints the outcome (ok | ud216 | oom | failed)
coh_run() { # series h steps np rep variant attempt
  local series=$1 h=$2 steps=$3 np=$4 rep=$5 v=$6 att=$7 c b t=1 p=bitwise opts=() extra=() camp outcome
  case "$v" in
    cpp) c=cpp ;;
    cppeig) c=cppeig ;;
    *) b=${v%%-*}; c=rust
       case "$v" in *-eig*) c=rusteig ;; esac
       case "$v" in *-fast*) p=fast ;; esac
       case "$v" in *-release*) extra+=(--rust-extra "rust_release_static_solver=true") ;; esac
       case "$v" in *-t[0-9]*) t=${v##*-t} ;; esac
       opts+=(--rust-bin "/target/final/$b/cohesive_extrinsic" --rust-volume "${FIN_TVOL}" --rust-image "${FIN_MPI_IMAGE}")
       [ "$t" = 1 ] || opts+=(--rust-threads "$t")
       [ "$p" = bitwise ] || opts+=(--rust-precision "$p") ;;
  esac
  camp="${series}-h${h}-np${np}-r${rep}-${v}"
  [ "$att" = 1 ] || camp="${camp}-a${att}"
  rm -rf "${D:?}/runs/${camp}"
  local partopt=()
  [ "$np" = 1 ] || partopt=(--partitions "$D/partitions/h$h")
  local cmd=(env OUT="$D/runs" PIN_CPUS_FROM=0 C1PREP_LOCK="$D/c1.lock" FNTE=joss CPP_CONFIG=strict C1_CONTAINER_PREFIX=final
    bash "$CAMPAIGN_TOOL" --memory "$memory" --cpus "$NCORES" ${partopt[@]+"${partopt[@]}"} ${opts[@]+"${opts[@]}"}
    ${extra[@]+"${extra[@]}"} "$camp" "$h" "$steps" "$np" 1 "$c")
  fin_cmdlog "$D" "${cmd[@]}"
  local t0 t1
  t0=$(date +%s)
  "${cmd[@]}" > "$D/runs/${camp}.log" 2>&1
  t1=$(date +%s)
  outcome=$(sed -n 's/^outcome="\(.*\)"$/\1/p' "$D/runs/$camp/$c/np$np/rep1/status.txt" 2> /dev/null)
  [ -n "$outcome" ] || outcome=failed
  # the run directories in the volumes are copies: free them
  docker run --rm -u 0 -v akantu-perf-cohesive:/perf -v "${FIN_TVOL}:/target" "${FIN_ORACLE_IMAGE}" \
    rm -rf "/perf/runs/$camp" "/target/c1prep/$camp" > /dev/null 2>&1
  python3 -I -c 'import json,sys; k=["series","h","steps","np","rep","variant","case","threads","precision","attempt","campaign","outcome","utc_start","wall_s"]
v=sys.argv[1:]; d=dict(zip(k,v)); [d.__setitem__(x,int(d[x])) for x in ("steps","np","rep","threads","attempt")]; d["wall_s"]=float(d["wall_s"])
print(json.dumps(d))' "$series" "$h" "$steps" "$np" "$rep" "$v" "$c" "$t" "$p" "$att" "$camp" "$outcome" \
    "$(date -u -d "@$t0" +%FT%TZ)" "$((t1 - t0))" >> "$D/index.jsonl"
  echo "$outcome"
}

# a group: the variants of one point in one repetition, order rotated by the
# repetition; repeated (at most twice) when a C++ run took upstream's MUMPS
# retry path (UD-216); then the checks
coh_group() { # series h steps np rep variants...
  local series=$1 h=$2 steps=$3 np=$4 rep=$5; shift 5
  local mark="$D/.done/${series}-h${h}-np${np}-r${rep}" order v att=1 res ud allok
  [ -e "$mark" ] && return 0
  mkdir -p "$D/.done" "$D/runs"
  coh_partition "$h" "$np" || return 0
  read -r -a order <<< "$(fin_rotate "$((rep - 1))" "$@")"
  while :; do
    ud=0 allok=1
    for v in "${order[@]}"; do
      local tv=1; case "$v" in *-t[0-9]*) tv=${v##*-t} ;; esac
      [ $((np * tv)) -le "$NCORES" ] || { fin_progress "$D" "${series} h${h} np${np}: ${v} needs $((np * tv)) cores: skipped"; continue; }
      res=$(coh_run "$series" "$h" "$steps" "$np" "$rep" "$v" "$att")
      [ "$res" = ok ] || allok=0
      [ "$res" = ud216 ] && ud=1
      fin_progress "$D" "${series} h${h} np${np} r${rep} ${v}: ${res}$([ "$att" = 1 ] || echo " (attempt ${att})")"
    done
    if [ "$ud" = 1 ] && [ "$att" -lt 3 ]; then att=$((att + 1)); continue; fi
    break
  done
  if [ "$allok" = 1 ]; then
    coh_check "$series" "$h" "$np" "$rep" "$att" "${order[@]}" && touch "$mark"
  else
    echo "$(fin_utc) ${series} h${h} np${np} r${rep}" >> "$D/FAILED"
  fi
}

# thread count and precision recorded by every port run; at one rank (and in
# the SparseLU series at every rank count) the per-step energies and the
# insertions of every bitwise variant identical (and equal to C++ at one rank)
coh_check() { # series h np rep attempt variants...
  local series=$1 h=$2 np=$3 rep=$4 att=$5; shift 5
  python3 -I - "$D" "$series" "$h" "$np" "$rep" "$att" "$@" <<'PY'
import json, os, sys
D, series, h, np, rep, att = sys.argv[1:7]
variants = sys.argv[7:]
bad, files = [], {}
for v in variants:
    camp = f"{series}-h{h}-np{np}-r{rep}-{v}" + ("" if att == "1" else f"-a{att}")
    case = "cpp" if v == "cpp" else "cppeig" if v == "cppeig" else ("rusteig" if "-eig" in v else "rust")
    d = os.path.join(D, "runs", camp, case, f"np{np}", "rep1")
    if not os.path.isdir(d):
        continue
    if not case.startswith("cpp"):
        want_t = int(v.rsplit("-t", 1)[1]) if "-t" in v else 1
        want_p = "fast" if "-fast" in v else "bitwise"
        s = json.load(open(os.path.join(d, "summary.json")))
        ex = s.get("rust_exec", {"threads": 1, "precision": "bitwise", "absent": True})
        if ex.get("threads") != want_t or ex.get("precision") != want_p:
            bad.append(f"{v}: rust_exec {ex}, requested threads {want_t} precision {want_p}")
        if want_p == "fast":
            continue
    e = os.path.join(d, "energies.csv")
    i = os.path.join(d, "insertions.csv")
    files[v] = (open(e, "rb").read() if os.path.exists(e) else None, open(i, "rb").read() if os.path.exists(i) else None)
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

# a series over points "H:NP" with the variants VARIANTS and STEPS steps;
# EXTRA_NP2 / EXTRA_NP4: additional variants at 2 / 4 ranks on the mesh STRONG
coh_series() { # series steps "points" "variants" [strong-h extra-np2 extra-np4] [fixed-reps]
  local series=$1 steps=$2 points=$3 variants=$4 strong=${5:-} x2=${6:-} x4=${7:-} nrep=${8:-$reps} r pt h np vs
  want_series "$series" || return 0
  fin_progress "$D" "series ${series}: ${steps} steps, points ${points}, variants ${variants}"
  for ((r = 1; r <= nrep; r++)); do
    # point order reversed every other repetition (as scaling_series.sh)
    local pts; read -r -a pts <<< "$points"
    if [ $((r % 2)) = 0 ]; then local rev=(); for ((i = ${#pts[@]} - 1; i >= 0; i--)); do rev+=("${pts[$i]}"); done; pts=("${rev[@]}"); fi
    for pt in "${pts[@]}"; do
      h=${pt%%:*} np=${pt#*:}
      [ "$np" -le "$NCORES" ] || continue
      read -r -a vs <<< "$variants"
      if [ "$h" = "$strong" ]; then
        [ "$np" = 2 ] && [ -n "$x2" ] && vs+=("$x2")
        [ "$np" = 4 ] && [ -n "$x4" ] && vs+=("$x4")
      fi
      coh_group "$series" "$h" "$steps" "$np" "$r" "${vs[@]}"
    done
  done
}

# ----------------------------------------------------------------------------
part_c() {
  if [ "$size" = full ]; then
    coh_series ws 500 "0.12:1 0.0925:1 0.0925:2 0.0756:2 0.0756:4 0.06:1 0.06:2 0.06:4 0.06:8 0.0476:8" "cpp before head" 0.06 head-t4 head-t2
    coh_series ins 1500 "0.12:1 0.12:2 0.12:4 0.12:8" "cpp before head" 0.12 head-t4 head-t2
    coh_series full12 5000 "0.12:1 0.12:2 0.12:4 0.12:8" "cpp before head" 0.12 head-t4 head-t2
    coh_series reh6 5000 "0.06:1 0.06:2 0.06:4 0.06:8" "cpp before head" 0.06 head-t4 head-t2
    coh_series reh4 300 "0.04:1 0.04:2 0.04:4 0.04:8" "cpp before head" 0.04 head-t4 head-t2
    coh_series eig 1500 "0.12:1 0.12:2 0.12:4 0.12:8" "before-eig head-eig" 0.12 head-eig-t4 head-eig-t2 1
  else
    coh_series ws 100 "0.12:1 0.12:2 0.12:4 0.12:8" "cpp before head" 0.12 head-t4 head-t2
    coh_series ins 800 "0.12:1 0.12:2 0.12:4 0.12:8" "cpp before head" 0.12 head-t4 head-t2
    coh_series full12 1000 "0.12:1 0.12:8" "cpp before head"
    coh_series reh6 200 "0.12:1 0.12:2 0.12:4 0.12:8" "cpp before head" 0.12 head-t4 head-t2
    coh_series reh4 50 "0.12:1 0.12:2 0.12:4 0.12:8" "cpp before head" 0.12 head-t4 head-t2
    coh_series eig 800 "0.12:1 0.12:2 0.12:4 0.12:8" "before-eig head-eig" 0.12 head-eig-t4 head-eig-t2 1
  fi
}

part_d() {
  local hs="0.12 0.06" sz=m threaded l labels=(before head) r np
  [ "$size" = dry ] && { hs="0.12"; sz=s; }
  threaded=$(fin_variant_key "$HEAD" sparselu_threaded)
  fin_progress "$D" "D: the head's SparseLU factorisation uses threads: ${threaded}"
  echo "sparselu_threaded=${threaded}" >> "$D/provenance.txt"
  local eigv="cppeig before-eig head-eig" h pts=""
  [ "$threaded" = true ] && eigv="${eigv} head-eig-t2 head-eig-t4 head-eig-t8"
  [ "$threaded" = true ] && labels+=(head-t2 head-t4 head-t8)
  for h in $hs; do pts="${pts} ${h}:1"; done
  coh_series eig1 10 "${pts# }" "$eigv"
  pts=""
  for h in $hs; do pts="${pts} ${h}:1 ${h}:2 ${h}:4"; done
  coh_series mumps 10 "${pts# }" "cpp before head"
  if want_series suite; then
    for ((r = 0; r < reps; r++)); do
      for l in static_linear_eigen static_linear_faer; do
        suite_serial_group "$D/suite-solid" suite_solid "$l" "$sz" "$r" "${labels[@]}"
      done
      for np in 1 2 4; do
        suite_mpi_group "$D/mpi-solid" suite_solid_mumps static_linear_mumps "$sz" "$np" "$r" before head
      done
    done
  fi
}

part_e() {
  if [ "$size" = full ]; then
    coh_series mem4 300 "0.04:1 0.04:2 0.04:4 0.04:8" "cpp before head head-release"
  else
    coh_series mem4 50 "0.12:1 0.12:2 0.12:4 0.12:8" "cpp before head head-release"
  fi
}

part_f() {
  local szl=l r np
  [ "$size" = dry ] && szl=s
  if [ "$size" = full ]; then
    coh_series setup 1 "0.06:1 0.06:2 0.06:4 0.06:8 0.04:1 0.04:2 0.04:4 0.04:8" "cpp before head"
  else
    coh_series setup 1 "0.12:1 0.12:2 0.12:4 0.12:8" "cpp before head"
  fi
  if want_series dist; then
    for ((r = 0; r < reps; r++)); do
      for np in 2 4 8; do
        [ "$np" -le "$NCORES" ] || continue
        suite_mpi_group "$D/dist-l" suite_setup dist_mesh_tet4 "$szl" "$np" "$r" before head
      done
    done
  fi
  want_series mpilayer && part_f_mpilayer
}

# the MPI layer against raw MPI (docs/benchmarks/mpi-latency-2026-11-14):
# method A, separate launches of mpi_bench (before, head) and bench_baseline
# (raw), rotated; method B, `mpi_bench calibrated` (one job per case)
part_f_mpilayer() {
  local out="$D/mpilayer" r btl cs np kind bytes it order l line us
  local cases=("2 pingpong 8 20000" "2 pingpong 65536 5000" "2 pingpong 1048576 500" "2 pingpong 16777216 30" "2 halo 8192 5000" "4 halo 8192 5000")
  [ "$size" = dry ] && cases=("2 pingpong 8 2000" "2 pingpong 1048576 50" "4 halo 8192 500")
  mkdir -p "$out/.done"
  [ -s "$out/separate.tsv" ] || printf 'rep\tbtl\tnp\tkind\tbytes\tprog\tus\n' > "$out/separate.tsv"
  run_in_mpi() { # np prog-dir prog args... (akantu-rs-mpi:dev, cores 0..np-1)
    local n=$1 pd=$2; shift 2
    docker run --rm --init --cpuset-cpus "0-$((n - 1))" --shm-size 2g -v "$pd:/b:ro" akantu-rs-mpi:dev \
      timeout -k 5 300 mpirun --allow-run-as-root "$@"
  }
  for ((r = 1; r <= reps; r++)); do
    for btl in sm tcp; do
      for cs in "${cases[@]}"; do
        read -r np kind bytes it <<< "$cs"
        local key="A.r$r.$btl.$np.$kind.$bytes"
        [ -e "$out/.done/$key" ] && continue
        read -r -a order <<< "$(fin_rotate "$((r - 1))" raw before head)"
        local rows=() okk=1
        for l in "${order[@]}"; do
          if [ "$l" = raw ]; then
            line=$(run_in_mpi "$np" "${FIN_BIN}/raw" --mca btl "$btl,self" -np "$np" /b/bench_baseline "$kind" "$bytes" "$it" 2>> "$out/err.log" | grep -a '^BASE' | head -1)
            us=$(printf '%s\n' "$line" | sed -n 's/.*half-RTT \([0-9.]*\) us.*/\1/p; s/.*: \([0-9.]*\) us per exchange.*/\1/p')
          else
            line=$(run_in_mpi "$np" "$(vdir "$l")/mpi" --mca btl "$btl,self" -np "$np" /b/mpi_bench "$kind" "$bytes" "$it" 2>> "$out/err.log" | grep -a '^PERF' | head -1)
            echo "$l $line" >> "$out/perf-lines.txt"
            us=$(printf '%s\n' "$line" | grep -o '[0-9.]* us' | head -1 | cut -d' ' -f1)
          fi
          [ -n "$us" ] || okk=0
          rows+=("$(printf '%s\t%s\t%s\t%s\t%s\t%s\t%s' "$r" "$btl" "$np" "$kind" "$bytes" "$l" "${us:-NA}")")
        done
        if [ "$okk" = 1 ]; then printf '%s\n' "${rows[@]}" >> "$out/separate.tsv"; touch "$out/.done/$key"
          fin_progress "$D" "mpilayer A r${r} ${btl} ${kind} ${bytes} B np${np}: done"
        else echo "$(fin_utc) mpilayer $key" >> "$D/FAILED"; fin_progress "$D" "mpilayer $key: FAILED (err.log)"; fi
      done
    done
  done
  # method B: one job per case and variant, raw MPI and akantu-comm interleaved in it
  local mode
  for ((r = 1; r <= reps; r++)); do
    for l in before head; do
      for mode in sm tcp; do
        local key="B.r$r.$l.$mode"
        [ -e "$out/.done/$key" ] && continue
        if run_in_mpi 4 "$(vdir "$l")/mpi" --mca btl "$mode,self" -np 2 /b/mpi_bench calibrated > "$out/calibrated.$l.$mode.r$r.txt" 2>> "$out/err.log"; then
          touch "$out/.done/$key"; fin_progress "$D" "mpilayer B r${r} ${l} ${mode}: done"
        else echo "$(fin_utc) mpilayer $key" >> "$D/FAILED"; fin_progress "$D" "mpilayer $key: FAILED"; fi
      done
    done
  done
}

part_g() {
  local mesh=joss steps=5000 pts="${gpoints}" pt np t vs v r
  if [ "$size" = dry ]; then mesh=0.06; steps=200; [ -n "$pts" ] || pts="8:head,cpp 4x2:head"
  else [ -n "$pts" ] || pts="64:head,cpp 32:head,cpp 32x2:head 8:head"; fi
  if [ "$mesh" = joss ]; then
    docker run --rm -v akantu-perf-cohesive:/perf:ro "${FIN_ORACLE_IMAGE}" test -f /perf/meshes/cube-joss.msh \
      || fin_die "part G: /perf/meshes/cube-joss.msh missing (prepare.sh --joss-mesh FILE)"
  fi
  want_series full || return 0
  # the partitions first (C++ export runs, stopped once partition.csv is complete)
  for pt in $pts; do np=${pt%%:*}; np=${np%%x*}; coh_partition "$mesh" "$np" || true; done
  for ((r = 1; r <= reps; r++)); do
    for pt in $pts; do
      np=${pt%%:*}; t=1
      case "$np" in *x*) t=${np#*x}; np=${np%%x*} ;; esac
      IFS=, read -r -a vs <<< "${pt#*:}"
      [ "$t" = 1 ] || { local tv=(); for v in "${vs[@]}"; do tv+=("$v-t$t"); done; vs=("${tv[@]}"); }
      coh_group full "$mesh" "$steps" "$np" "$r" "${vs[@]}"
    done
  done
}

case "$part" in
  A) part_a ;;
  B) part_b ;;
  C) part_c ;;
  D) part_d ;;
  E) part_e ;;
  F) part_f ;;
  G) part_g ;;
esac

fin_scrub_tree "$D"
status=0
if [ -s "$D/FAILED" ]; then fin_progress "$D" "part ${part}: failed runs (FAILED)"; status=1; fi
if [ -s "$D/CHECKS_FAILED" ]; then fin_progress "$D" "part ${part}: failed checks (CHECKS_FAILED)"; status=1; fi
if ! fin_guard_tree "$D" > "$D/.guard" 2>&1; then fin_progress "$D" "part ${part}: the data names the machine (.guard)"; status=1; fi
python3 -I "${FIN_HERE}/summarize.py" "$D" > "$D/.summarize.log" 2>&1 || { fin_progress "$D" "part ${part}: summarize.py failed (.summarize.log)"; status=1; }
fin_progress "$D" "part ${part} end: status ${status}"
exit "$status"
