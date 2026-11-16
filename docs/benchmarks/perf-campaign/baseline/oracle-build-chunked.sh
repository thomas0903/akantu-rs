#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): how the baseline built
# the two oracle installs on the shared measurement machine (the record of
# the procedure; docs/ORACLE_BUILD.md describes the build itself).
#
# Oracle builds in chunks: every chunk is one bjob running ninja -j2 for at
# most 20 min inside the oracle image (ninja is incremental, ccache keeps
# objects), so other agents' tjobs run between chunks. Only the installed
# targets (akantu, py11_akantu and googletest's gtest, gtest_main: the only
# install rules are the library, the Python module, the CMake package files
# and the fetched googletest); upstream's test and example executables are
# not built. Then configure-build.sh --install-only and
# finalize-install.sh. A chunk that fails (not a timeout) is retried once
# with -j1 (a translation unit killed by the OOM killer).
# Not itself under a wrapper.
set -uo pipefail
# shellcheck disable=SC2016 # $AKANTU_JOB_CPUS expands in the inner shell, inside the job wrapper
export AKANTU_SRC_HOST=$HOME/w/infra/akantu-ref
cd ~/w/infra/akantu-rs || exit 1
. oracle/scripts/common.sh
for c in strict default; do
  if [ ! -f "$HOME/w/infra/oracle-configured-$c" ]; then
    if bjob infra-oracle-$c-configure -- sh -c 'DOCKER_CPUSET="$AKANTU_JOB_CPUS" exec "$@"' sh oracle/scripts/configure-build.sh $c -j2 --configure-only --foreground; then
      touch "$HOME/w/infra/oracle-configured-$c"
    else
      echo "configure $c failed"; exit 1
    fi
  fi
  k=0 j=2 retried=0
  while :; do
    k=$((k + 1))
    echo "=== $c chunk $k start $(date -u +%FT%TZ)"
    # the bjob's cores (AKANTU_JOB_CPUS, known only inside the wrapper)
    bjob infra-oracle-$c-chunk$k -- sh -c 'exec docker run --cpuset-cpus "$AKANTU_JOB_CPUS" "$@"' sh \
      --rm --name akantu-oracle-chunk-$c "${ORACLE_RUN_OPTS[@]}" "${ORACLE_MOUNTS[@]}" "${ORACLE_IMAGE}" \
      bash -c "timeout -s INT 20m ninja -C /build/$c -j $j -k 0 akantu py11_akantu gtest gtest_main > /build/logs/$c/chunk$k.log 2>&1; rc=\$?; tail -2 /build/logs/$c/chunk$k.log; exit \$rc"
    rc=$?
    echo "=== $c chunk $k rc=$rc $(date -u +%FT%TZ)"
    [ "$rc" = 0 ] && break
    if [ "$rc" != 124 ] && [ "$rc" != 130 ]; then
      [ "$retried" = 0 ] || { echo "$c: library targets failed twice (rc=$rc)"; exit 1; }
      retried=1 j=1
    fi
  done
  echo "=== $c install $(date -u +%FT%TZ)"
  bjob infra-oracle-$c-install -- sh -c 'DOCKER_CPUSET="$AKANTU_JOB_CPUS" exec "$@"' sh oracle/scripts/configure-build.sh $c --install-only --foreground; echo "install $c exit $?"
  echo "=== $c finalize $(date -u +%FT%TZ)"
  bjob infra-oracle-$c-finalize -- sh -c 'DOCKER_CPUSET="$AKANTU_JOB_CPUS" exec "$@"' sh oracle/scripts/finalize-install.sh $c; echo "finalize $c exit $?"
done
echo "=== done $(date -u +%FT%TZ)"
