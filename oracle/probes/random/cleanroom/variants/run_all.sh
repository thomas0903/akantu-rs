#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs. Runs every alternative of the clean-room variant harness
# (D-030) against the committed fixtures; see README.md.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
python3 -I make_variants.py
cargo build --release --quiet "$@"
bin=target/release/akantu-rng-cleanroom-variants
for a in "" uniform=1 uniform=2 exp=1 exp=2 n_flip=1 n_mult=1 n_mult=2 n_ret=1 \
         g_a2=1 g_a2=2 g_a1=1 g_vloop=1 g_vloop=2 g_pre=1 g_sq=1 g_sq=2 g_sq=3 \
         g_lt=1 g_lt=2 g_lt=3 g_ret=1 g_small=1 g_small=2 g_small=3 g_small=4 \
         cauchy=1 f_order=1 f_expr=1 f_expr=2 f_expr=3 f_expr=4 t_order=1 t_expr=1 \
         t_expr=2 weibull=1 weibull=2 ev=1; do
  "$bin" "$a"
done
