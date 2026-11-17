#!/bin/sh
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs cohesive scalability harness (not ported from Akantu): binds one
# MPI rank to its own cores, then runs it. Started by mpirun for every rank,
# with mpirun's own binding off (`--bind-to none`):
#
#   mpirun -np P --bind-to none sh bind-rank.sh T CMD [ARG...]
#
# Rank r (OMPI_COMM_WORLD_RANK) is bound to the r-th block of T cores of the
# cores the process may use (the container's cpuset, in ascending order): the
# placement of `--map-by core --bind-to core` (T = 1) and of `--map-by
# slot:PE=T --bind-to core`. It replaces them because the launcher of
# Open MPI 5.0.7 fails to start more than about 15 bound ranks on one host
# (`prterun: SIGPIPE detected`, seen on a 128-core host at 16 ranks and more;
# 8 bound ranks start).
t=${1:?threads per rank}; shift
r=${OMPI_COMM_WORLD_RANK:-${PMIX_RANK:-}}
[ -n "$r" ] || { echo "bind-rank.sh: no rank in the environment" >&2; exit 97; }
list=$(awk '/^Cpus_allowed_list:/ {print $2}' /proc/self/status)
cores=$(echo "$list" | tr ',' '\n' | awk -v r="$r" -v t="$t" '
  { n = split($0, a, "-"); lo = a[1] + 0; hi = (n > 1 ? a[2] : a[1]) + 0
    for (c = lo; c <= hi; c++) all[k++] = c }
  END { if ((r + 1) * t > k) exit 1
        s = all[r * t]; for (i = 1; i < t; i++) s = s "," all[r * t + i]; print s }') \
  || { echo "bind-rank.sh: rank $r with $t core(s) does not fit in $list" >&2; exit 98; }
exec taskset -c "$cores" "$@"
