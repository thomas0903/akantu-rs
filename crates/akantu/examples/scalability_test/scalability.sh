#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Ported from Akantu (EPFL/LSMS): examples/c++/scalability_test/scalability.sh
# (Copyright (©) 2010-2023 EPFL (Ecole Polytechnique Fédérale de Lausanne),
# Laboratory (LSMS - Laboratoire de Simulation en Mécanique des Solides);
# Akantu is free software under the GNU Lesser General Public License,
# version 3 or later).
#
# Submits the scalability study of the Rust example `scalability_test`
# (EX-049): one SLURM job (run.sh) per process count, 72 cores per node as
# in upstream's study (`--qos serial` up to one node, `--qos parallel`
# beyond). Extra arguments go to `sbatch` (e.g. `--account`). The akantu-rs
# project never submits it (no cluster); the scripts are kept for users.

job_name="${JOB_NAME:-elastic_rust}"

for n in 1 2 4 9 18 36 72 144 288 432 576 648 720; do
    if [ $n -le 72 ]; then
        QOS="--qos serial"
        node=1
        npn=$n
    else
        QOS="--qos parallel"
        node=$(( $n / 72 ))
        npn=72
    fi
    # shellcheck disable=SC2086 # QOS is a word list
    sbatch -J ${job_name} -e ${job_name}_%A.err -o ${job_name}_%A.out -N ${node} --ntasks-per-node ${npn} $QOS "$@" --exclusive ./run.sh
done
