#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: runs cli_probe (Akantu cppargparse) on every
# (scenario, parse flags, command line) case below and prints one JSON line
# per case: exit status, the probe's JSON output (success), the error message
# and whether the help text was printed (the help text itself is recorded once
# per scenario in a "help" line). Run inside akantu-oracle:dev:
#   python3 cli_cases.py /path/to/cli_probe > cli_blackbox.jsonl
import json
import subprocess
import sys

CASES = {
    "float": [
        ["--val", "1.5"], ["--val", "1e400"], ["--val", "-1e400"],
        ["--val", "1e-400"], ["--val", "-1e-400"], ["--val", "2e308"],
        ["--val", "1.7976931348623157e308"], ["--val", "1.7976931348623159e308"],
        ["--val", "1e"], ["--val", "1e+"], ["--val", "1e5x"], ["--val", "-.5"],
        ["--val", "."], ["--val", "nan"], ["--val", "inf"], ["--val", "0x10"],
        ["--val", ""], ["--val", " 3"], ["--val", "-3"], ["--val", "--val"],
        ["--val", "4", "extra"], ["extra", "--val", "4"], ["--val"], [],
        ["--val", "1", "--val", "2"], ["--val", "5.", "rest", "-x"],
        ["--val", "4.9e-324"], ["--val", "2.4e-324"], ["--val", "1e-310"],
        ["--val", "1,5"], ["--val", "+1"], ["--val", "1e+5"], ["--val", "-"],
    ],
    "float_def": [
        [], ["--opt"], ["--opt", "2"], ["--opt", "x"], ["--opt", "1e400"],
        ["--val", "1e400"], ["--opt", "--val", "3"], ["--val", "7", "--opt"],
        ["-h"], ["--help", "--val", "3"], ["--opt", "1e"], ["--val", "-0"],
    ],
    "float_pos": [
        ["1.5"], ["1e400"], ["1e400", "2"], ["1.5", "2.5"], ["1.5", "x"],
        ["x", "1.5"], ["1.5", "2.5", "3.5"], [], ["-1", "-2"], ["1e", "2"],
        ["1.5", "1e400"], ["1e-400", "-1e-400"],
    ],
    "int": [
        ["-n", "3"], ["-n", "-4"], ["-n", "99999999999999999999"],
        ["-n", "-99999999999999999999"], ["-n", "3.5"], ["-n", "x"],
        ["-n", "0x10"], ["-n", " 7"], ["--number", "+5"], ["--many"],
        ["--many", "1", "2", "3"], ["--many", "1", "x", "2"],
        ["--many", "1", "-n", "2"], ["-n", "1", "-n", "2"], ["--many", "1e3"],
        ["-n", "9223372036854775807"], ["-n", "-9223372036854775808"],
        ["-n", "9223372036854775808"], ["-n", "-"], ["-n", ""],
    ],
    "bool": [
        ["--b", "1"], ["--b", "0"], ["--b", "2"], ["--b", "true"],
        ["--b", "-1"], ["--b", "x"], ["--flag"], ["--flag", "1"], [], ["--b"],
        ["--b", "01"], ["--b", "1x"], ["--b", "99999999999999999999"],
    ],
    "mixed": [
        ["a", "b"], ["a", "--val", "1e400"], ["--val", "1e400", "a"],
        ["a", "--unknown"], ["--unknown", "a"], ["a", "b", "--opt"],
        ["a", "--opt", "x", "-n", "2"], ["-n", "2"], ["a", "-n", "1e400"],
        ["a", "--val", "1e-400", "-n", "-3", "b"], ["--opt", "--opt"],
        ["a", "--val"], ["a", "--val", "1e", "b"], ["a", "-n", "x", "b"],
        ["a", "", "b"], ["a", "-h"],
    ],
    "akantu": [
        ["mesh.msh", "--aka_debug_level", "5", "--aka_print_backtrace",
         "--user_opt", "3"],
        ["--aka_input_file", "x.dat", "--aka_seed", "12"],
        ["--aka_seed", "1e400"], ["--aka_debug_level"], ["--aka_input_file"],
        ["-h"], ["--aka_seed", "99999999999999999999"],
        ["--aka_seed", "-5", "--aka_seed", "3"], ["a", "b", "c"],
    ],
}


def run(probe, scenario, flags, args):
    return subprocess.run([probe, scenario, str(flags)] + args,
                          capture_output=True)


def main():
    probe = sys.argv[1]
    for scenario, lines in CASES.items():
        # cppargparse prints the help text after every error message (and
        # alone for -h): record it once per scenario.
        help_text = run(probe, scenario, 1, ["-h"]).stderr.decode()
        if not help_text.startswith("Usage:"):  # required positional missing
            help_text = help_text.partition("\n\n")[2]
        print(json.dumps({"kind": "help", "scenario": scenario,
                          "text": help_text}, sort_keys=True))
        for flags in range(4):
            for args in lines:
                r = run(probe, scenario, flags, args)
                out = r.stdout.decode()
                err = r.stderr.decode()
                msg = None
                if err and err != help_text:
                    msg, sep, rest = err.partition("\n\n")
                    assert sep and rest == help_text, (scenario, args, err)
                print(json.dumps({
                    "kind": "case",
                    "scenario": scenario,
                    "flags": flags,
                    "args": args,
                    "exit": r.returncode,
                    "out": json.loads(out) if r.returncode == 0 and out else None,
                    "help": bool(err),
                    "msg": msg,
                }, sort_keys=True))


if __name__ == "__main__":
    main()
