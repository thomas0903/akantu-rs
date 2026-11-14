#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Runs INSIDE the akantu-oracle:dev container (called by configure-build.sh).
# Usage: build.sh <config> <jobs> <mode>
#   mode: configure | build | install | all   (all = configure + build + install;
#         build = build + install; install = install only)
# A failing test/example target does not prevent installation as long as the
# library targets (akantu, py11_akantu) are complete; failed targets are
# listed in /build/logs/<config>/failed-targets.txt and the script exits
# non-zero.
# Layout: source /akantu-src (ro) or /build/src-<config> when patches exist,
#         build tree /build/<config>, install prefix /build/install-<config>,
#         logs /build/logs/<config>/.
set -euo pipefail

config="${1:?config name}"
jobs="${2:-2}"
mode="${3:-all}"

cfg_file="/oracle/configs/${config}.cmake"
[ -f "${cfg_file}" ] || { echo "unknown config ${config} (${cfg_file})" >&2; exit 2; }

build_dir="/build/${config}"
install_dir="/build/install-${config}"
log_dir="/build/logs/${config}"
mkdir -p "${build_dir}" "${log_dir}" /build/ccache

stamp() { date -u +%Y-%m-%dT%H:%M:%SZ; }
log() { echo "[$(stamp)] $*" | tee -a "${log_dir}/steps.log"; }

# --- Source tree: pristine reference, or a patched copy -------------------
src_dir=/akantu-src
shopt -s nullglob
patches=(/oracle/patches/*.patch)   # oracle/patches/unused/ is NOT applied
shopt -u nullglob
if [ ${#patches[@]} -gt 0 ]; then
  src_dir="/build/src-${config}"
  if [ "${mode}" = configure ] || [ "${mode}" = all ]; then
    # Patch a fresh staging copy, then sync it into ${src_dir} by content
    # (no times): unchanged files keep their mtimes, so re-configuring does
    # not trigger a full rebuild.
    stage="/build/src-${config}.stage"
    log "copying reference to ${stage} and applying ${#patches[@]} patch(es)"
    rsync -a --delete /akantu-src/ "${stage}/"
    for p in "${patches[@]}"; do
      log "applying $(basename "$p")"
      patch -d "${stage}" -p1 --forward < "$p"
    done
    mkdir -p "${src_dir}"
    rsync -rlpD --checksum --delete "${stage}/" "${src_dir}/"
    rm -rf "${stage}"
  fi
fi

sha="$(git -C /akantu-src rev-parse HEAD)"
log "config=${config} jobs=${jobs} mode=${mode} src=${src_dir} sha=${sha}"

if [ "${mode}" = configure ] || [ "${mode}" = all ]; then
  # --fresh: always configure from the initial cache file only, never from a
  # stale CMakeCache.txt (reproducibility).
  log "configure start"
  /usr/bin/time -v -o "${log_dir}/configure.time" \
    cmake --fresh -C "${cfg_file}" -G Ninja \
      -S "${src_dir}" -B "${build_dir}" \
      -DCMAKE_INSTALL_PREFIX="${install_dir}" \
      2>&1 | tee "${log_dir}/configure.log"
  log "configure done"
  cp "${build_dir}/CMakeCache.txt" "${log_dir}/CMakeCache.txt"
fi

if [ "${mode}" = build ] || [ "${mode}" = all ]; then
  log "build start (ninja -j${jobs})"
  set +e
  /usr/bin/time -v -o "${log_dir}/build.time" \
    ninja -C "${build_dir}" -j "${jobs}" -k 0 2>&1 | tee "${log_dir}/build.log"
  rc=${PIPESTATUS[0]}
  if [ "${rc}" -ne 0 ]; then
    # A TU may have been OOM-killed with parallel jobs: retry serially, keep
    # going to collect every genuine error.
    log "build failed (rc=${rc}); retrying with -j1"
    /usr/bin/time -v -o "${log_dir}/build-j1.time" \
      ninja -C "${build_dir}" -j 1 -k 0 2>&1 | tee "${log_dir}/build-j1.log"
    rc=${PIPESTATUS[0]}
  fi
  set -e
  log "build done rc=${rc}"
  grep -h '^FAILED: ' "${log_dir}/build.log" "${log_dir}/build-j1.log" 2>/dev/null \
    | sed 's/^FAILED: //' | sort -u > "${log_dir}/failed-targets.txt" || true
  build_rc=${rc}
  if [ "${rc}" -ne 0 ]; then
    log "failed targets: $(wc -l < "${log_dir}/failed-targets.txt") (see failed-targets.txt)"
    if ninja -C "${build_dir}" -n akantu py11_akantu | grep -q 'no work to do'; then
      log "library targets complete: installing anyway"
    else
      log "library targets incomplete: not installing"
      exit "${rc}"
    fi
  fi
fi

if [ "${mode}" = build ] || [ "${mode}" = all ] || [ "${mode}" = install ]; then
  log "install start"
  cmake --install "${build_dir}" 2>&1 | tee "${log_dir}/install.log"
  # Record the flags out-of-tree consumers must use to match this build.
  mkdir -p "${install_dir}/share/akantu-oracle"
  {
    echo "config=${config}"
    echo "reference_sha=${sha}"
    grep -E '^(CMAKE_BUILD_TYPE|CMAKE_CXX_FLAGS|CMAKE_CXX_FLAGS_RELWITHDEBINFO|CMAKE_C_FLAGS|AKANTU_EXTRA_CXX_FLAGS|CMAKE_CXX_COMPILER):' \
      "${build_dir}/CMakeCache.txt"
  } > "${install_dir}/share/akantu-oracle/build-info.txt"
  cp "${cfg_file}" "/oracle/configs/common.cmake" "${install_dir}/share/akantu-oracle/" 2>/dev/null || true
  log "install done"
fi
exit "${build_rc:-0}"
