#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: runs INSIDE akantu-oracle:dev (started by
# oracle/scripts/generate-fixtures.sh). Builds the probes and writes fixtures.
#
# Usage: run-in-container.sh <config> <installed|standalone> <target>...
# Targets: selftest element fe_engine fe_extra cl_random core_init parsable
#          mesh mesh:<case_id> material
#          material:<case_id> model model:<case_id> regression:<case_id>
# Mounts: /akantu-src (ro), /build (ro, oracle volume), /probes (ro),
#         /fixtures (rw, output fixtures root), /frozen-meshes (ro, optional:
#         fixtures/meshes, Gmsh-generated meshes frozen per D-009).
#
# Every probe writes into a private staging directory. Only when every
# target (including the mesh determinism re-run) succeeded are the staged
# cases promoted into /fixtures and the size policy applied
# (finalize_fixtures.py); otherwise /fixtures is left untouched.
set -euo pipefail

config="${1:?config}"
mode="${2:?mode}"
shift 2
targets=("$@")
SRC=/akantu-src
PROBES=/probes
OUT=/fixtures
prefix="/build/install-${config}"
work="$(mktemp -d /tmp/akantu-probes.XXXXXX)"
stage="${work}/stage"
mkdir -p "${stage}"
log() { echo "[generate-fixtures] $*"; }
: "${AKANTU_PROBE_REFERENCE_SHA:?set by generate-fixtures.sh}"
: "${AKANTU_PROBE_CREATED:?set by generate-fixtures.sh}"

case "${mode}" in
  installed)
    [ -f "${prefix}/READY" ] || { echo "no ${prefix}/READY marker: install not usable yet" >&2; exit 3; }
    flags_file="${prefix}/share/akantu-oracle/build-info.txt"
    [ -f "${flags_file}" ] || { echo "no ${flags_file}: cannot verify the installed oracle" >&2; exit 3; }
    ;;
  standalone)
    flags_file="/build/${config}/CMakeCache.txt"
    [ -f "${flags_file}" ] || { echo "no configured tree /build/${config}" >&2; exit 3; }
    ;;
  *) echo "mode must be installed|standalone" >&2; exit 2 ;;
esac

# Provenance: installed oracle identity (refused on mismatch), probe sources,
# patched source tree with per-file content hashes (O1 build.sh creates
# /build/src-<config> when oracle/patches/*.patch exist).
src_tree="${SRC}"
base_args=(--expect-sha "${AKANTU_PROBE_REFERENCE_SHA}" --probe-tree "${PROBES}"
           --install-prefix "${prefix}")
[ -n "${ORACLE_IMAGE_ID:-}" ] && base_args+=(--expect-image "${ORACLE_IMAGE_ID}")
[ -f "/build/${config}/compile_commands.json" ] &&
  base_args+=(--library-compile-commands "/build/${config}/compile_commands.json")
[ "${AKANTU_ALLOW_IMAGE_MISMATCH:-0}" = 1 ] && base_args+=(--allow-image-mismatch)
if [ -d "/build/src-${config}" ]; then
  src_tree="/build/src-${config}"
  base_args+=(--reference-tree "${SRC}" --patched-tree "${src_tree}" --patches-out "${work}/patches.txt")
fi
python3 -I "${PROBES}/python/build_env.py" base "${config}" "${mode}" "${flags_file}" \
  "${work}/build_env.json" "${base_args[@]}"
if [ -f "${work}/patches.txt" ]; then
  AKANTU_PROBE_PATCHES="$(cat "${work}/patches.txt")"
  export AKANTU_PROBE_PATCHES
fi
export AKANTU_PROBE_CONFIG="${config}"
fp_flags="$(sed -nE 's/^CMAKE_CXX_FLAGS(:[A-Z]+)?=(.*)$/\2/p' "${flags_file}" | head -1)"
log "config=${config} mode=${mode} fp_flags='${fp_flags}' src=${src_tree} stage=${stage}"

bin="${work}/build"
mkdir -p "${bin}"
probe_info() { echo "${work}/build_env.$1.json"; }
if [ "${mode}" = installed ]; then
  cgal_dir="$(sed -n 's/^CGAL_DIR:PATH=//p' "${flags_file}" | head -1)"
  cmake -S "${PROBES}" -B "${bin}" -G Ninja \
    -DCMAKE_PREFIX_PATH="${prefix}" -DCMAKE_BUILD_TYPE=RelWithDebInfo \
    ${cgal_dir:+-DCGAL_DIR="${cgal_dir}"} \
    -DAKANTU_PROBE_CONFIG="${config}" -DAKANTU_PROBE_CXX_FLAGS="${fp_flags}" >"${work}/cmake.log"
  # Only the material probe when every target is a material target (the
  # other probes take ~25 min at -j1; their provenance below only reads
  # compile_commands.json).
  ninja_targets=(material_probe)
  for t in "${targets[@]}"; do
    case "${t}" in material|material:*) ;; *) ninja_targets=() ;; esac
  done
  ninja -C "${bin}" -j1 ${ninja_targets[@]+"${ninja_targets[@]}"} >"${work}/ninja.log" ||
    { tail -50 "${work}/ninja.log"; exit 1; }
  for p in element_probe mesh_probe material_probe; do
    python3 -I "${PROBES}/python/build_env.py" probe "${work}/build_env.json" "$(probe_info "${p}")" \
      "${p}" --compile-commands "${bin}/compile_commands.json" --source-suffix "cpp/${p}.cc"
  done
  python3 -I "${PROBES}/python/build_env.py" probe "${work}/build_env.json" "$(probe_info fe_engine_probe)" \
    fe_engine_probe --compile-commands "${bin}/compile_commands.json" --source-suffix "fe/fe_engine_probe.cc"
  # Probes
  python3 -I "${PROBES}/python/build_env.py" probe "${work}/build_env.json" "$(probe_info fe_extra_probe)" \
    fe_extra_probe --compile-commands "${bin}/compile_commands.json" --source-suffix "fe/fe_extra_probe.cc"
  for p in law_random_probe init_probe parsable_probe; do
    python3 -I "${PROBES}/python/build_env.py" probe "${work}/build_env.json" "$(probe_info "${p}")" \
      "${p}" --compile-commands "${bin}/compile_commands.json" --source-suffix "cpp/${p}.cc"
  done
  python3 -I "${PROBES}/python/build_env.py" probe "${work}/build_env.json" \
    "$(probe_info writer_selftest)" writer_selftest \
    --compile-commands "${bin}/compile_commands.json" --source-suffix "selftest/writer_selftest.cc"
else
  # Header-only probes only: compile with the exact include/define flags of
  # the configured library tree (compile_commands.json) and link the three
  # translation units they need (see cpp/standalone_globals.cc).
  incs="$(python3 -I - "/build/${config}/compile_commands.json" <<'EOF'
import json, shlex, sys
cc = json.load(open(sys.argv[1]))
e = next(c for c in cc if c["file"].endswith("src/fe_engine/gauss_integration.cc"))
args = shlex.split(e["command"])
keep = [a for a in args if a.startswith(("-I", "-D")) and "akantu_EXPORTS" not in a
        and "AKANTU_MODULE" not in a]
print(" ".join(keep))
EOF
)"
  bt_flags="$(sed -nE 's/^CMAKE_CXX_FLAGS_RELWITHDEBINFO(:[A-Z]+)?=(.*)$/\2/p' "${flags_file}" | head -1)"
  # shellcheck disable=SC2086
  cxx=(mpicxx -std=c++20 ${bt_flags} ${fp_flags} -I"${PROBES}/include" -I"/build/${config}/src"
       ${incs} -isystem /usr/include/eigen3
       "-DAKANTU_PROBE_CONFIG=\"${config}\""
       "-DAKANTU_PROBE_CXX_FLAGS=\"${bt_flags} ${fp_flags} (standalone)\""
       "-DAKANTU_PROBE_BUILD_TYPE=\"RelWithDebInfo\"")
  "${cxx[@]}" -c "${PROBES}/cpp/element_probe.cc" -o "${bin}/element_probe.o"
  "${cxx[@]}" -c "${src_tree}/src/fe_engine/gauss_integration.cc" -o "${bin}/gauss_integration.o"
  "${cxx[@]}" -c "${src_tree}/src/common/aka_error.cc" -o "${bin}/aka_error.o"
  "${cxx[@]}" -c "${PROBES}/cpp/standalone_globals.cc" -o "${bin}/standalone_globals.o"
  mpicxx "${bin}"/element_probe.o "${bin}"/gauss_integration.o "${bin}"/aka_error.o \
    "${bin}"/standalone_globals.o -o "${bin}/element_probe"
  # shellcheck disable=SC2086
  selftest_cxx=(g++ -std=c++17 ${bt_flags} ${fp_flags} -I"${PROBES}/include"
                "${PROBES}/selftest/writer_selftest.cc" -o "${bin}/writer_selftest")
  "${selftest_cxx[@]}"
  python3 -I "${PROBES}/python/build_env.py" probe "${work}/build_env.json" \
    "$(probe_info element_probe)" element_probe \
    --command "$(printf '%q ' "${cxx[@]}") -c ${PROBES}/cpp/element_probe.cc -o ${bin}/element_probe.o"
  python3 -I "${PROBES}/python/build_env.py" probe "${work}/build_env.json" \
    "$(probe_info writer_selftest)" writer_selftest --command "$(printf '%q ' "${selftest_cxx[@]}")"
fi

python_path_for_akantu() {
  # O1 records the module directory in build-info.txt; fall back to a search.
  local d
  d="$(sed -n 's/^python_module_dir=//p' "${prefix}/share/akantu-oracle/build-info.txt" 2>/dev/null | head -1)"
  if [ -n "${d}" ] && [ -d "${d}" ]; then echo "${d}"; return; fi
  d="$(find "${prefix}" -type d -name akantu -path '*-packages/akantu' 2>/dev/null | head -1)"
  [ -n "${d}" ] && dirname "${d}"
}

# Mesh sources (mesh_probe paths are relative to ${MESH_ROOT}): every
# committed reference mesh of test/, examples/, extra_packages/ and
# third-party/iohelper/test/, plus the Gmsh-generated meshes frozen in
# fixtures/meshes (D-009) under the prefix generated/ (mounted at
# /frozen-meshes by generate-fixtures.sh; see oracle/probes/mesh_io/).
MESH_ROOT="${work}/mesh-root"
mkdir -p "${MESH_ROOT}"
for d in test examples extra_packages third-party; do
  ln -s "${SRC}/${d}" "${MESH_ROOT}/${d}"
done
frozen_dir="$(ls -d /frozen-meshes/gmsh-* 2>/dev/null | LC_ALL=C sort | tail -1 || true)"
[ -n "${frozen_dir}" ] && ln -s "${frozen_dir}" "${MESH_ROOT}/generated"

# Mesh files whose fixture case id is one of "$@" (all meshes without
# arguments); same id rule as mesh_probe.cc case_id_for.
select_meshes() {
  (cd "${MESH_ROOT}" && {
     find -L test examples extra_packages third-party/iohelper/test -name '*.msh'
     [ -e generated ] && find -L generated -name '*.msh'
   } | LC_ALL=C sort) | python3 -I -c '
import re, sys
wanted = set(sys.argv[1:])
def cid(rel):
    rel = rel[5:] if rel.startswith("test/") else rel
    rel = rel[:-4] if rel.endswith(".msh") else rel
    return "mesh_io/" + re.sub(r"[^A-Za-z0-9_.+/-]", "_", rel)
paths = [l.strip() for l in sys.stdin if l.strip()]
hit = [p for p in paths if not wanted or cid(p) in wanted
       or any(re.fullmatch(re.escape(cid(p)) + r"_dim[123]", w) for w in wanted)]
missing = [w for w in wanted if not any(cid(p) == w or re.fullmatch(re.escape(cid(p)) + r"_dim[123]", w) for p in paths)]
if missing:
    sys.exit("no reference mesh for case(s): " + ", ".join(missing))
print("\n".join(hit))
' "$@"
}

status=0
complete=()
mesh_cases=()
mesh_all=0
model_cases=()
model_all=0
material_cases=()
material_all=0
for t in "${targets[@]}"; do
  case "${t}" in
    mesh) mesh_all=1 ;;
    mesh:*) mesh_cases+=("${t#mesh:}") ;;
    model) model_all=1 ;;
    model:model/*) model_cases+=("${t#model:model/}") ;;
    material) material_all=1 ;;
    material:material/*) material_cases+=("${t#material:material/}") ;;
  esac
done
if [ "${mode}" != installed ]; then
  for t in "${targets[@]}"; do
    case "${t}" in
      selftest|element) ;;
      *) echo "target ${t} needs the installed library (mode ${mode})" >&2; exit 2 ;;
    esac
  done
fi

run_target() {
  local t="$1"
  case "${t}" in
    selftest)
      complete+=(selftest)
      AKANTU_PROBE_BUILD_INFO="$(probe_info writer_selftest)" "${bin}/writer_selftest" "${stage}" || return 1
      python3 -I "${PROBES}/python/build_env.py" python "${work}/build_env.json" \
        "$(probe_info writer_selftest_py)" "${PROBES}/selftest/writer_selftest.py" || return 1
      AKANTU_PROBE_BUILD_INFO="$(probe_info writer_selftest_py)" \
        python3 -I "${PROBES}/selftest/writer_selftest.py" "${stage}" || return 1
      ;;
    element)
      # area `fe` is only complete when fe_engine and fe_extra run in the same
  # invocation
      ran_element=1
      AKANTU_PROBE_BUILD_INFO="$(probe_info element_probe)" "${bin}/element_probe" "${stage}" || return 1
      ;;
    fe_engine)
      ran_fe_engine=1
      AKANTU_PROBE_BUILD_INFO="$(probe_info fe_engine_probe)" "${bin}/fe_engine_probe" "${stage}" "${SRC}" || return 1
      ;;
    fe_extra)
      # Fixtures/fe/engine_extra (part of area fe)
      ran_fe_extra=1
      AKANTU_PROBE_BUILD_INFO="$(probe_info fe_extra_probe)" "${bin}/fe_extra_probe" "${stage}" "${SRC}" || return 1
      ;;
    cl_random)
      complete+=(cl)
      AKANTU_PROBE_BUILD_INFO="$(probe_info law_random_probe)" "${bin}/law_random_probe" "${stage}" "${work}/cl_random" || return 1
      ;;
    core_init)
      complete+=(core)
      AKANTU_PROBE_BUILD_INFO="$(probe_info init_probe)" \
        python3 -I "${PROBES}/python/init_cases.py" "${bin}/init_probe" "${stage}" || return 1
      ;;
    parsable)
      complete+=(parser)
      AKANTU_PROBE_BUILD_INFO="$(probe_info parsable_probe)" "${bin}/parsable_probe" "${stage}" "${work}/parsable" || return 1
      ;;
    material)
      complete+=(material)
      AKANTU_PROBE_BUILD_INFO="$(probe_info material_probe)" \
        python3 -I "${PROBES}/python/material_cases.py" "${bin}/material_probe" "${stage}" || return 1
      ;;
    regression:*)
      AKANTU_PROBE_BUILD_INFO="$(probe_info material_probe)" \
        python3 -I "${PROBES}/python/replay_regression.py" "${bin}" "${OUT}" "${stage}" "${t#regression:}" || return 1
      ;;
    *) echo "unknown target ${t}" >&2; return 2 ;;
  esac
}

run_mesh() {
  local meshes
  if [ "${mesh_all}" = 1 ]; then
    mapfile -t meshes < <(select_meshes)
  else
    mapfile -t meshes < <(select_meshes "${mesh_cases[@]}") || return 1
  fi
  [ ${#meshes[@]} -gt 0 ] || { echo "no mesh selected" >&2; return 1; }
  AKANTU_PROBE_BUILD_INFO="$(probe_info mesh_probe)" "${bin}/mesh_probe" "${stage}" "${MESH_ROOT}" "${meshes[@]}" || return 1
  # Determinism check: an independent second run must succeed and agree
  # with the first, except for reviewed allow-listed reference defects.
  if ! AKANTU_PROBE_BUILD_INFO="$(probe_info mesh_probe)" "${bin}/mesh_probe" "${work}/rerun" "${MESH_ROOT}" "${meshes[@]}" >"${work}/rerun.log" 2>&1; then
    tail -20 "${work}/rerun.log" >&2
    echo "mesh: the determinism re-run failed; nothing is promoted" >&2
    return 1
  fi
  python3 -I "${PROBES}/python/drop_nondeterministic.py" "${stage}/mesh_io" "${work}/rerun/mesh_io" \
    "${MESH_ROOT}" --allow-list "${PROBES}/python/known_nondeterminism.json" || return 1
}

run_model() {
  local pp
  pp="$(python_path_for_akantu)"
  [ -n "${pp}" ] || { echo "akantu python package not found under ${prefix}" >&2; return 1; }
  python3 -I "${PROBES}/python/build_env.py" python "${work}/build_env.json" \
    "$(probe_info model_probe)" "${PROBES}/python/model_probe.py" --module-dir "${pp}" || return 1
  local cases=()
  [ "${model_all}" = 1 ] || cases=("${model_cases[@]}")
  AKANTU_PROBE_BUILD_INFO="$(probe_info model_probe)" PYTHONPATH="${pp}" \
    LD_LIBRARY_PATH="${prefix}/lib:${LD_LIBRARY_PATH:-}" \
    python3 "${PROBES}/python/model_probe.py" "${stage}" "${SRC}" "${cases[@]}"
}

done_mesh=0
done_model=0
done_material=0
for t in "${targets[@]}"; do
  log "target ${t}"
  case "${t}" in
    mesh|mesh:*)
      [ "${done_mesh}" = 0 ] || continue
      done_mesh=1
      [ "${mesh_all}" = 1 ] && complete+=(mesh_io)
      run_mesh || status=1
      ;;
    model|model:model/*)
      [ "${done_model}" = 0 ] || continue
      done_model=1
      [ "${model_all}" = 1 ] && complete+=(model)
      run_model || status=1
      ;;
    material:material/*)
      [ "${material_all}" = 1 ] && continue
      [ "${done_material}" = 0 ] || continue
      done_material=1
      AKANTU_PROBE_BUILD_INFO="$(probe_info material_probe)" \
        python3 -I "${PROBES}/python/material_cases.py" "${bin}/material_probe" "${stage}" \
        "${material_cases[@]}" || status=1
      ;;
    model:*|material:*)
      echo "target ${t}: expected model:model/<case> or material:material/<case>" >&2
      status=2
      ;;
    *)
      run_target "${t}" || status=$?
      ;;
  esac
done

if [ "${ran_element:-0}" = 1 ] && [ "${ran_fe_engine:-0}" = 1 ] && [ "${ran_fe_extra:-0}" = 1 ]; then
  complete+=(fe)
fi
if [ "${status}" != 0 ]; then
  echo "generation failed (status ${status}): nothing was promoted into ${OUT}" >&2
  exit "${status}"
fi
fin_args=(--stage "${stage}" --created "${AKANTU_PROBE_CREATED}")
for a in "${complete[@]}"; do fin_args+=(--complete-area "${a}"); done
python3 -I "${PROBES}/python/finalize_fixtures.py" "${OUT}" "${fin_args[@]}"
