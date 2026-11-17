#!/bin/sh
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs cohesive scalability harness (not ported from Akantu): binds one
# MPI rank to its own cores, then runs it. Started by mpirun for every rank,
# with mpirun's own binding off (`--bind-to none`):
#
#   mpirun -np P --bind-to none sh bind-rank.sh T CMD [ARG...]
#
# The rank's index on its host, r (OMPI_COMM_WORLD_LOCAL_RANK; without it
# OMPI_COMM_WORLD_RANK or PMIX_RANK, the same number when every rank runs on
# one host), selects the r-th block of T cores of the cores the process may
# use (the container's cpuset, in ascending order): the placement of
# `--map-by core --bind-to core` (T = 1) and of `--map-by slot:PE=T
# --bind-to core`, on one host and on each host of a multi-host job.
#
# Why not mpirun's binding: with `--bind-to core`, Open MPI 5.0.7 (PRRTE
# 3.0) aborts the start of more than 10 bound ranks per host inside a Docker
# container with the default seccomp profile (`prterun: SIGPIPE detected -
# aborting`). Each forked child binds its memory with set_mempolicy(), which
# that profile refuses without CAP_SYS_NICE (EPERM); the child then sends the
# "memory not bound" warning over its status pipe, whose read end the parent
# has already closed: EPIPE, SIGPIPE, and the child, not yet exec'd, still
# runs prterun's SIGPIPE handler, which signals prterun's own event loop.
# prterun aborts at the 11th SIGPIPE (10 bound ranks start, 11 fail, on any
# core count). `docker run --cap-add SYS_NICE` (or no seccomp filter) removes
# the failure; binding with taskset needs neither.
t=${1:?threads per rank}; shift
r=${OMPI_COMM_WORLD_LOCAL_RANK:-${OMPI_COMM_WORLD_RANK:-${PMIX_RANK:-}}}
[ -n "$r" ] || { echo "bind-rank.sh: no rank in the environment" >&2; exit 97; }
list=$(awk '/^Cpus_allowed_list:/ {print $2}' /proc/self/status)
cores=$(echo "$list" | tr ',' '\n' | awk -v r="$r" -v t="$t" '
  { n = split($0, a, "-"); lo = a[1] + 0; hi = (n > 1 ? a[2] : a[1]) + 0
    for (c = lo; c <= hi; c++) all[k++] = c }
  END { if ((r + 1) * t > k) exit 1
        s = all[r * t]; for (i = 1; i < t; i++) s = s "," all[r * t + i]; print s }') \
  || { echo "bind-rank.sh: rank $r with $t core(s) does not fit in $list" >&2; exit 98; }
exec taskset -c "$cores" "$@"
