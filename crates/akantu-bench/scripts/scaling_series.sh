#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: interleaved scaling series of the cohesive
# scalability test (both codes), built on c1prep_campaign.sh. Runs on a
# Linux host with Docker.
#
#   bash crates/akantu-bench/scripts/scaling_series.sh [--memory M] [--cpus N] \
#        SERIES STEPS REPS "H:NP H:NP ..." "CASES" [key=value ...]
#
#   SERIES  name prefix of every campaign of the series ([A-Za-z0-9._-])
#   STEPS   time steps of every run (500: before the first insertion on the
#           benchmark meshes, so every run does the same work per step)
#   REPS    repetitions of every point (>= 5 for timings on a shared VM)
#   H:NP    the points: mesh h (as c1prep_campaign.sh's H: 0.12 = the frozen
#           mesh, another h = /perf/meshes/cube-h<H>.msh from gen-meshes.sh)
#           and the rank count. One series can hold the points of several
#           scaling studies (a weak series = h chosen so that elements / NP
#           stays nearly constant; a strong series = one h, several NP):
#           measuring them in the same repetitions puts every point under
#           the same drift of the machine.
#   CASES   e.g. "cpp rust" (c1prep_campaign.sh's case names)
#   key=value  extra user-section parameters for both codes
#
# What it does:
#   1. per point with NP > 1, one C++ partition export (c1prep_campaign.sh
#      --export-partition; AKA_SEED = 1, one Scotch thread), kept in
#      OUT/SERIES-parts/h<H>/partition-np<NP>.csv: every repetition of the
#      point, both codes, uses the same partition (the C++ runs re-create it
#      and compare: `partition_matches`; the Rust runs replay it);
#   2. REPS repetitions; repetition k runs every point once, the points in
#      the given order for odd k and reversed for even k, and per point the
#      cases rotated by k (adjacent A / B runs, neither code always first):
#      one campaign per (point, repetition), named SERIES-h<H>-np<NP>-r<k>,
#      run by c1prep_campaign.sh with REPS = 1 (one run at a time, memory
#      sampler, outcome per run);
#   3. scaling_analyze.py OUT SERIES -> OUT/SERIES-summary.{md,json}.
#
# Options and environment as c1prep_campaign.sh (--memory, default 14g; --cpus,
# default 8; OUT, default ~/c1prep-results; CPP_CONFIG strict; FNTE joss;
# PIN_CPUS_FROM).
# SERIES_REPS (optional, e.g. "2"): run only these repetitions of step 2
# (and step 3 over whatever is done), so that a series can run as one short
# exclusive job per repetition on a shared host.
# SKIP_DONE=1 (optional): a campaign whose schedule.log already says
# "campaign done" is not run again (resuming a series after the host
# stopped); an incomplete campaign directory is moved aside to
# <campaign>.incomplete-<time> and run again.
# Exit status: 0 iff every campaign and the analysis succeeded; 1 otherwise
# (the series continues after a failed campaign and lists it at the end).
set -uo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
camp="${here}/c1prep_campaign.sh"

opts=()
while [ $# -gt 0 ]; do
  case "$1" in
    --memory|--cpus) opts+=("$1" "${2:?}"); shift 2 ;;
    -h|--help) sed -n '7,40p' "$0"; exit 2 ;;
    --*) echo "unknown option $1" >&2; exit 2 ;;
    *) break ;;
  esac
done
[ $# -ge 5 ] || { sed -n '7,40p' "$0"; exit 2; }
series=$1 steps=$2 reps=$3 points=$4 cases=$5; shift 5
extra=("$@")
[[ "${series}" =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,40}$ ]] || { echo "invalid series name '${series}'" >&2; exit 2; }
[[ "${reps}" =~ ^[1-9][0-9]?$ ]] || { echo "invalid REPS '${reps}'" >&2; exit 2; }
read -r -a point_list <<< "${points}"
read -r -a case_list <<< "${cases}"
[ "${#point_list[@]}" -ge 1 ] && [ "${#case_list[@]}" -ge 1 ] || { echo "no points or no cases" >&2; exit 2; }
for p in "${point_list[@]}"; do
  [[ "${p}" =~ ^[0-9.]+:[1-9][0-9]{0,2}$ ]] || { echo "invalid point '${p}' (H:NP)" >&2; exit 2; }
done
out="${OUT:-${HOME}/c1prep-results}"
mkdir -p "${out}"
log="${out}/${series}-series.log"
say() { echo "$(date -u +%FT%TZ) $*" | tee -a "${log}"; }
say "series ${series}: steps ${steps}, reps ${reps}, points ${points}, cases ${cases}, options ${opts[*]:-} extra ${extra[*]:-}"

failed=""
# ---- 1. partitions --------------------------------------------------------
for p in "${point_list[@]}"; do
  h=${p%%:*} np=${p##*:}
  [ "${np}" -gt 1 ] || continue
  pdir="${out}/${series}-parts/h${h}"
  [ -s "${pdir}/partition-np${np}.csv" ] && continue
  pc="${series}-part-h${h}-np${np}"
  if bash "${camp}" ${opts[@]+"${opts[@]}"} --export-partition "${pc}" "${h}" "${np}"; then
    mkdir -p "${pdir}" && cp "${out}/${pc}/partition-np${np}.csv" "${pdir}/partition-np${np}.csv"
    say "partition h=${h} np=${np}: $(sha256sum "${pdir}/partition-np${np}.csv" | cut -c1-16)…"
  else
    say "partition h=${h} np=${np}: export FAILED"
    failed="${failed} part-h${h}-np${np}"
  fi
done

# ---- 2. repetitions ---------------------------------------------------------
nc=${#case_list[@]}
for rep in $(seq 1 "${reps}"); do
  case " ${SERIES_REPS:-${rep}} " in *" ${rep} "*) ;; *) continue ;; esac
  order=("${point_list[@]}")
  if [ $((rep % 2)) -eq 0 ]; then
    order=()
    for ((i = ${#point_list[@]} - 1; i >= 0; i--)); do order+=("${point_list[$i]}"); done
  fi
  rot=()
  for i in $(seq 0 $((nc - 1))); do rot+=("${case_list[$(( (i + rep - 1) % nc ))]}"); done
  say "repetition ${rep}: points ${order[*]}, cases ${rot[*]}"
  for p in "${order[@]}"; do
    h=${p%%:*} np=${p##*:}
    c="${series}-h${h}-np${np}-r${rep}"
    if [ "${SKIP_DONE:-0}" = 1 ] && [ -d "${out}/${c}" ]; then
      if grep -q "campaign done" "${out}/${c}/schedule.log" 2> /dev/null; then
        say "  ${c}: already done, skipped (SKIP_DONE=1)"
        continue
      fi
      mv "${out}/${c}" "${out}/${c}.incomplete-$(date +%s)"
    fi
    popt=()
    [ "${np}" -gt 1 ] && popt=(--partitions "${out}/${series}-parts/h${h}")
    if bash "${camp}" ${opts[@]+"${opts[@]}"} ${popt[@]+"${popt[@]}"} "${c}" "${h}" "${steps}" "${np}" 1 "${rot[*]}" ${extra[@]+"${extra[@]}"} \
         > "${out}/${c}.log" 2>&1; then
      say "  ${c}: ok"
    else
      say "  ${c}: FAILED (exit $?; ${out}/${c}/schedule.log)"
      failed="${failed} ${c}"
    fi
  done
done

# ---- 3. analysis --------------------------------------------------------------
python3 -I "${here}/scaling_analyze.py" "${out}" "${series}" > "${out}/${series}-analysis.log" 2>&1
arc=$?
if [ -n "${failed}" ] || [ "${arc}" -ne 0 ]; then
  say "series ${series} FAILED:${failed:- none}; analysis rc=${arc}"
  exit 1
fi
say "series ${series} done: every campaign ok; ${out}/${series}-summary.md"
exit 0
