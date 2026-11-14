# Complex division clean-room contract (D-030 amendment)

`Complex::quotient(self, other) -> Complex` (and the private helpers it
needs) was removed because its previous derivation used inspection of the
oracle's compiled libgcc `__divdc3` machine code, which D-030 (amended) does
not permit for GPL(+exception) code.

Re-implement it **without** reading GCC/libgcc sources, without
disassembling/inspecting compiled libgcc code, and without reading the removed
implementation (no `git log -p` / `git show` / `git diff` / `git blame` of
`dense/complex.rs` before this stub commit, on any branch).

Allowed: Smith (1962) "Algorithm 116: Complex division"; Baudin & Smith (2012)
"A robust complex division in Scilab"; ISO C11 Annex G (G.5.1 division
special cases); IEEE-754; black-box experiments: compile and run small C++
programs dividing `std::complex<double>` values in `akantu-oracle:dev`
(g++ 14.2, `-O2 -std=c++20 -ffp-contract=off`), observing outputs bit for bit
(including special values, scaling/overflow/underflow cases). Whether the
oracle's division uses fused multiply-adds may be established by black-box
experiments only (e.g. inputs where fused and unfused results differ).

Acceptance: the existing dense parity fixtures
(`crates/akantu-linalg/tests/fixtures/dense/`, including the complex-division
records) pass bit-exactly; keep a provenance log
`crates/akantu-linalg/src/dense/COMPLEX_CLEANROOM_LOG.md`.

## Status (2026-11-10, A3c)

Re-implemented under these rules. The
algorithm, the evidence for every choice and the compliance statement are in
`COMPLEX_CLEANROOM_LOG.md`; probes and their outputs are in
`oracle/probes/dense/complex_cleanroom/`. The `dense_strict.txt` `cdiv`
records and the new `complex_div_cleanroom.txt` fixture (4500 black-box
records) pass bit for bit, except NaN sign/payload, which is not part of
the contract.
