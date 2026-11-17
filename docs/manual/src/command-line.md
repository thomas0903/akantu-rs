# Command-line and environment options

Upstream programs call `akantu::initialize("input.dat", argc, argv)`
(`src/common/aka_common.cc`), which creates the communicator, registers
and parses Akantu's command-line options, reads the input file into the
static parser and seeds the random generator. The port provides the same
pieces:

| Upstream | Rust |
|---|---|
| `getStaticArgumentParser()` with the `--aka_*` options | `akantu::parser::cli::ArgumentParser` + `register_akantu_options` |
| `readInputFile(file)`, `getStaticParser()` | `akantu::runtime::parse_input(file)`, `akantu::runtime::global()` |
| seed policy of `initialize` | `akantu::core::random::initial_seed` + `RandomGenerator::seed` |

The single call that chains these steps is `akantu::runtime::initialize`
(parity row CORE-038). A program that needs only some of them performs the
steps itself, as shown below.

## Options

<!-- BEGIN GENERATED: aka-options -->
```text
$ my_simulation --help
Usage: my_simulation [--aka_debug_level AKA_DEBUG_LEVEL] [--aka_input_file AKA_INPUT_FILE] [--aka_print_backtrace] [--aka_seed AKA_SEED] [-h]

optional arguments:
  --aka_debug_level AKA_DEBUG_LEVEL  Akantu's overall debug level (0: error, 1: exceptions, 4: warnings, 5: info, ..., 100: dump more info on levels can be found in aka_error.hh) (default: 4)
  --aka_input_file AKA_INPUT_FILE  Akantu's input file (default: )
  --aka_print_backtrace  Should Akantu print a backtrace in case of error (default: false) (const: true)
  --aka_seed AKA_SEED  The seed to use on prank 0
  -h, --help       show this help message and exit (default: false) (const: true)
```
<!-- END GENERATED: aka-options -->

As upstream, the options are parsed with the `_remove_parsed` flag: the
recognised arguments are removed from the argument list and unknown ones
are left for the application.

```rust
use akantu::parser::cli::{register_akantu_options, ArgumentParser, ParseFlags, ParseOutcome};

let mut options = ArgumentParser::new();
register_akantu_options(&mut options)?;

let mut argv: Vec<String> = ["my_simulation", "--aka_seed", "42", "--mesh", "bar.msh"]
    .iter().map(|s| s.to_string()).collect();
let outcome = options.parse_argv(&mut argv, ParseFlags::REMOVE_PARSED, true)?;
assert_eq!(outcome, ParseOutcome::Parsed);
assert_eq!(options.get_int("aka_seed")?, 42);
assert_eq!(options.get_int("aka_debug_level")?, 4); // default: warnings
assert_eq!(options.get_string("aka_input_file")?, "");
assert!(!options.get_bool("aka_print_backtrace")?);
// the application's own arguments are left in place
assert_eq!(argv, ["my_simulation", "--mesh", "bar.msh"]);
# Ok::<(), akantu::core::Error>(())
```

Differences from upstream: on a malformed command line upstream prints
the error and the help text (on rank 0) and calls `exit`; the port
returns `Error::InvalidArgument`. For `-h`/`--help` it returns
`ParseOutcome::HelpRequested`; the caller prints
`ArgumentParser::help_string()` and exits.

## Input file

`--aka_input_file <file>` replaces the file name the program passes to
`initialize` (an empty value means "use the program's file"). The file is
then read into the process-global parser:

```rust
use akantu::parser::cli::{register_akantu_options, ArgumentParser, ParseFlags};
# let dir = std::env::temp_dir().join(format!("akantu-manual-cli-{}", std::process::id()));
# std::fs::create_dir_all(&dir)?;
# let default_file = dir.join("material.dat");
# let other_file = dir.join("other.dat");
# std::fs::write(&default_file, "a = 1\n")?;
# std::fs::write(&other_file, "a = 2\n")?;

let mut options = ArgumentParser::new();
register_akantu_options(&mut options)?;
let mut argv = vec![
    "prog".to_string(),
    "--aka_input_file".to_string(),
    other_file.display().to_string(),
];
options.parse_argv(&mut argv, ParseFlags::REMOVE_PARSED, true)?;

// upstream initialize(): an empty --aka_input_file keeps the program's file
let from_cli = options.get_string("aka_input_file")?;
let input = if from_cli.is_empty() { default_file.clone() } else { from_cli.into() };
akantu::runtime::parse_input(&input)?;

let runtime = akantu::runtime::global().read().unwrap();
assert_eq!(runtime.parser.root.parameter("a").unwrap().value, "2");
# drop(runtime);
# std::fs::remove_dir_all(&dir)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Random seed

The random generator used by random parameters is seeded with, in order of
precedence:

1. the environment variable `AKA_SEED` (read with C `atol`),
2. the option `--aka_seed`,
3. the global input-file parameter `seed`,
4. the current time (`time(nullptr)`),

and the value $s$ is multiplied by $r + 1$ on the rank $r$
(`rank + 1`), so every MPI rank draws a
different sequence (random fields therefore depend on the number of
ranks, as upstream). `initial_seed` implements this policy:

```rust
use akantu::core::random::{initial_seed, IdxSlot, RandomGenerator};

// (AKA_SEED, --aka_seed, input `seed`, time, rank)
assert_eq!(initial_seed(Some("10"), Some(20), Some(30), 1_700_000_000, 0), 10);
assert_eq!(initial_seed(None, Some(20), Some(30), 1_700_000_000, 0), 20);
assert_eq!(initial_seed(None, None, Some(30), 1_700_000_000, 2), 90); // 30 × (2 + 1)
assert_eq!(initial_seed(None, None, None, 1_700_000_000, 0), 1_700_000_000);

// upstream RandomGenerator<Idx>::seed(seed)
let seed = initial_seed(std::env::var("AKA_SEED").ok().as_deref(), Some(42), None, 0, 0);
RandomGenerator::<IdxSlot>::seed(seed);
```

## Debug level

`--aka_debug_level` (default 4, warnings) and the environment variable
`AKA_DEBUG_LEVEL` set upstream's debug printing level, and both take
effect in the port through `akantu::runtime::initialize`. As upstream
(`aka_common.cc`), `initialize` first sets the level to 0 (errors only),
then applies `--aka_debug_level`, then, when `AKA_DEBUG_LEVEL` is set, its
value (read with C `atoi`): the environment variable wins over the
option. The level that results is the one the debugger of
`akantu::core::debug` uses for its messages, is returned in
`Initialized::debug_level` and is what `akantu::core::debug::debug_level()`
reports afterwards. `initialize_with` takes the process inputs explicitly
(`ProcessInputs`), which makes the precedence testable without touching
the environment of the process:

```rust
use akantu::comm::Communicator;
use akantu::runtime::{initialize_with, ProcessInputs};

// the option alone: level 7
let mut argv: Vec<String> = ["prog", "--aka_debug_level", "7"]
    .iter().map(|s| s.to_string()).collect();
let inputs = ProcessInputs::default(); // no AKA_DEBUG_LEVEL, no AKA_SEED
let init = initialize_with("", &mut argv, &inputs, Communicator::serial())?;
assert_eq!(init.debug_level, 7);
assert_eq!(akantu::core::debug::debug_level().0, 7);
assert_eq!(argv, ["prog"]); // the parsed option is removed

// AKA_DEBUG_LEVEL wins over the option (glibc `atoi`: " 12abc" is 12)
let mut argv: Vec<String> = ["prog", "--aka_debug_level", "7"]
    .iter().map(|s| s.to_string()).collect();
let inputs = ProcessInputs {
    env_aka_debug_level: Some(" 12abc".to_string()),
    ..ProcessInputs::default()
};
let init = initialize_with("", &mut argv, &inputs, Communicator::serial())?;
assert_eq!(init.debug_level, 12);
assert_eq!(akantu::core::debug::debug_level().0, 12);
# Ok::<(), akantu::core::Error>(())
```

`--aka_print_backtrace` takes effect likewise: `initialize` passes it to
the debugger (`akantu::core::debug::set_print_backtrace`), which then
captures and prints the stack in its error and uncaught-error reports
(`akantu::core::debug::print_backtrace_enabled()` reads the flag back).
The frame text is Rust's, not glibc's (upstream defect UD-274, category
C). `RUST_BACKTRACE=1` additionally prints backtraces for panics.

The port reports warnings and information through the
[`log`](https://docs.rs/log) facade as well: `initialize` installs a
bridge that sends the `log` records of the readers and of the parser to
the debugger, so they are filtered by the same level, unless the
application has installed a logger of its own, which is then kept.

## Python: the options are read from `sys.argv` at import

The Python package needs no `initialize` call (its `akantu.initialize`
raises `RuntimeError`, as upstream's): `import akantu` runs the module
function `__initialize()`, which passes a copy of `sys.argv` to the same
steps as `initialize(argc, argv)` (upstream `py_aka_common.cc`,
`register_initialize`). A script started as

```text
python3 my_script.py --aka_debug_level 5 --aka_input_file material.dat --mesh bar.msh
```

therefore reads `material.dat` into the static parser (what
`akantu.getStaticParser()` returns, as after `akantu.parseInput`), sets the
debug level to 5 (`akantu.debug.getDebugLevel()`; `AKA_DEBUG_LEVEL` wins
over the option), passes `--aka_print_backtrace` to the debugger and seeds
the random generator with the policy above (`AKA_SEED`, `--aka_seed`, the
input file's `seed`, the time). Unknown arguments such as `--mesh bar.msh`
are ignored and `sys.argv` itself is not modified (the parsed options
are removed from the copy only), so the script parses its own options as
usual. A malformed akantu option (e.g. `--aka_debug_level x`) makes
`import akantu` raise `akantu.Exception` (exit status 1 if uncaught);
upstream's module aborts the interpreter instead (`SIGABRT`, exit status
134). The behaviour is pinned by
`python/tests/test_wp_py3.py` (`test_doc004_*`, run in fresh
interpreters).
