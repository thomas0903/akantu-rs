# MPI point-to-point latency on x86_64 (2026-11-14)

akantu-comm against raw MPI on x86_64, all message sizes, shared memory and
TCP. Results and reading: `docs/BENCHMARKS.md`, "Point-to-point latency
against raw MPI".

| File | Content |
|---|---|
| `provenance.txt` | date, commit, CPU, kernel, toolchain, Open MPI version and MCA settings of the image, SHA-256 of both binaries |
| `separate.tsv` | method A, one line per run: repetition, transport, ranks, case, bytes, program (`raw` = `scripts/mpi/bench_baseline.c` built with `mpicc -O2`; `akantu` = `crates/akantu-comm/examples/mpi_bench.rs`, release), microseconds (ping-pong: half round trip; halo: time per exchange, slowest rank) |
| `perf-lines.txt` | the `PERF` lines printed by `mpi_bench` in method A |
| `calibrated.txt` | method B, the `CAL summary` line of every `mpi_bench calibrated` job (raw MPI through the `mpi` crate and `akantu-comm` in interleaved blocks of one job; 9 blocks), 3 repetitions |
| `mpilat.sh`, `mpilat-host.sh` | the scripts that produced the files: `mpilat-host.sh` runs `mpilat.sh` in `akantu-rs-mpi:dev` through `scripts/mpi/in-docker.sh` (container 4 CPUs, 6 GiB) and copies the results out of the target volume. To repeat the measurement, run `mpilat-host.sh` from a checkout at `~/akantu-rs` with `mpilat.sh` in the home directory, or run `mpilat.sh` inside the image directly |

Machine: an x86_64 Linux virtual machine (boat.dev `default`), 4 vCPU of an AMD Ryzen 9 9950X host, Ubuntu 24.04, kernel 6.8.0-117,
image `akantu-rs-mpi:dev` `sha256:073f4a99…` (Open MPI 5.0.7, Rust
1.99.1). The image sets `OMPI_MCA_btl_sm_single_copy_mechanism=none`, so
the shared-memory transport copies in and out of a shared buffer; the development Mac's
series used single copy.
