# Parser clean-room contract (D-030)

`src/stream.rs` (C++ iostream conversion/formatting emulation) and the
distribution-printing part of `src/param.rs` (`impl Display for
RandomParameter`, `RandomDistribution::parameters_text`) were stripped to
signatures because the previous implementation followed
libstdc++ internals, and **re-implemented clean-room** in
an earlier commit. The provenance record is
[`CLEANROOM_LOG.md`](CLEANROOM_LOG.md).

Rules for anyone changing these items: do not read libstdc++/libc++ sources
or the removed code (no `git log -p`, `git show`, `git diff`, `git blame` on
these files in their history before the clean-room rewrite).
Allowed inputs: the ISO C++ standard ([facet.num.get.virtuals] stages 1–3,
[istream.formatted.arithmetic], [istream::sentry], [facet.num.put.virtuals],
[ostream.inserters.arithmetic], boolalpha/noboolalpha rules), the ISO C
standard (strtoll/strtoull/strtod and printf `%e`/`%g` conversion rules,
"C" locale), upstream Akantu sources (LGPL; `src/io/parser/**` show how
conversions are invoked), Boost sources (BSL-1.0, with notices), the
committed oracle fixtures, and black-box experiments compiling small C++
programs in the `akantu-oracle:dev` Docker image (running them is allowed;
reading the library headers is not). New behaviour must be backed by an
observation in `oracle/probes/parser/stream_blackbox.cc` /
`tests/fixtures/parser/stream_blackbox.jsonl` and logged in
`CLEANROOM_LOG.md`.

Public items (`src/stream.rs`): `is_c_space`, `trim_c_space`, `StreamInt`
(+ impls for i8, i16, i32, i64, u8, u16, u32, u64; `CHARACTER`/`from_byte`
for the character types), `IntExtraction<T>` (`value`, `fail`, `eof`,
`consumed`), `extract_int`, `extract_double`, `extract_bool`,
`extract_word`, `word_bytes`, `parse_int`, `parse_bool`, `parse_word`,
`string_list_bytes`, `parse_string_list`, `parse_string_set`,
`format_scientific`, `format_general`, `format_double`; in `src/param.rs`:
`RandomDistribution::parameters_text` and `impl Display for
RandomParameter`.
