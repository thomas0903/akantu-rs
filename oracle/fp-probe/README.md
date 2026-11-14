# FP contraction probe (evidence for D-031)

`run.sh` compiles `scalar.cc` (`a*b+c`) and `eigen.cc` (fixed 3x3 and dynamic
matrix products, dot, gemv with Eigen 3.4.0) with the oracle image's GCC and
counts fused multiply-add instructions (`fmadd/fmsub/fnmadd/fnmsub/fmla/fmls`)
in the object code.

    docker run --rm -v "$PWD/oracle/fp-probe:/work" akantu-oracle:dev bash /work/run.sh

Result on 2026-11-09 (GCC 14.2.0, aarch64, Eigen 3.4.0):

| flags | fused insns |
|---|---|
| scalar, `-std=gnu++20` (default contraction) | 1 |
| scalar, `-std=c++20` (ISO mode, still contracts for C++) | 1 |
| scalar, `-ffp-contract=off` (either std) | 0 |
| eigen, `-O2 -std=gnu++20` | 410 |
| eigen, `-O2 -std=gnu++20 -ffp-contract=off` | 357 |
| eigen, `-O2 -std=gnu++20 -ffp-contract=off -DEIGEN_DONT_VECTORIZE` | 0 |
| eigen, `-O2 -std=gnu++20 -DEIGEN_DONT_VECTORIZE` | 130 |

`-ffp-contract=off` alone leaves Eigen's NEON `pmadd` (`vfmaq_f64`, explicit
intrinsic) in place; only the combination removes every fused operation.
