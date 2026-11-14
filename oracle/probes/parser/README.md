# Parser oracle probe

`dump_parser.cc` loads an input file with the C++ reference `akantu::Parser`
(reference `35a851780d0b9471d258ed53a003ae98aaa95ea0`) and prints the whole
tree as JSON: section types, names and options; parameters in upstream
`std::map` order with raw value, line and column; sub-sections in upstream
`std::multimap` order; and, for every parameter, the outcome of every
upstream conversion (`Real`, `Vector<Real>`, `Matrix<Real>`,
`RandomParameter<Real>` with its `operator<<` output, `bool`, `Int`,
`UInt`, `std::string`, `std::vector<std::string>`). Doubles are IEEE-754
bit patterns. Each conversion runs in a forked child, so the stack overflow
that upstream hits on cyclic variable references (`a = a`, also present in
upstream `test_parser/input_file.dat`: `yop = yop`) is recorded as
`{"error": "crash"}` instead of aborting the dump. Error classes: `akantu`
(`debug::Exception`), `std` (other `std::exception`, i.e. Spirit X3
`expectation_failure`).

| File | Purpose |
|---|---|
| `dump_parser.cc` | the probe |
| `CMakeLists.txt` | builds it against `/build/install-strict` via `use-akantu-oracle.cmake` (oracle FP flags) |
| `run.sh` | host script: builds in `akantu-oracle:dev` (`-j2`), dumps every fixture to `crates/akantu-parser/tests/fixtures/parser/<path>.json`, writes `PROVENANCE.txt` |
| `gen_expressions.py` | generates `crates/akantu-parser/tests/fixtures/extra/expressions.dat` (literal/expression/vector/matrix/random/stream corpus) |
| `gen_extra.py` | generates the other `extra/*.dat` fixtures: deep nesting (130/5000 parentheses, 20000 signs, 257 sections, a 300-variable chain), cyclic references, bytes ≥ 0x80 |
| `stream_blackbox.cc` | clean-room black-box probe of the C++ standard library (D-030): stream extraction, number formatting, `<random>` distribution printing → `tests/fixtures/parser/stream_blackbox.jsonl` (see `crates/akantu-parser/CLEANROOM_LOG.md`) |
| `cli_probe.cc`, `cli_cases.py` | runs Akantu `cppargparse` on 115 command lines × 4 parse flags → `tests/fixtures/parser/cli_blackbox.jsonl` |
| `registry_probe.cc` | registers `std::string`, `ID` and `std::set<ID>` members with `ParameterRegistry::registerParam`, fills them with `Parsable::parseSection` from `tests/fixtures/registry/registry_bytes.dat` (bytes ≥ 0x80, trailing white space) and prints them hex encoded, with `getParameterValue<std::string>` and `getParameter(name, default)` → `tests/fixtures/parser/registry_bytes.json` (compared by `tests/bytes.rs`) |

Regenerate (requires `/build/install-strict/READY` in the
`akantu-oracle-build` volume):

```sh
python3 oracle/probes/parser/gen_expressions.py > crates/akantu-parser/tests/fixtures/extra/expressions.dat
python3 oracle/probes/parser/gen_extra.py crates/akantu-parser/tests/fixtures/extra
oracle/probes/parser/run.sh
cargo test -p akantu-parser --test oracle --test stream_blackbox --test cli_oracle --test bytes
```

The Rust side is `crates/akantu-parser/tests/oracle.rs` (exact comparison:
bitwise on Linux/glibc; on macOS bitwise except the enumerated
`LIBM_EXCEPTIONS`, see its documentation and
`docs/NUMERICAL_VALIDATION.md`), `stream_blackbox.rs`, `cli_oracle.rs` and
`bytes.rs`.
