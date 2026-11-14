<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Security policy

## Reporting a vulnerability

Report security problems privately, through the repository's private
vulnerability reporting: on GitHub, the "Security" tab, then "Report a
vulnerability". The report opens a draft advisory that only you and the
maintainer can see. Please do not open a public issue for a security problem.

Include what is affected (crate, function, file format), the version or
commit, a minimal input or program that shows the problem, and what an
attacker gains. The project has one maintainer, who answers in the advisory as
soon as time allows and agrees with you on when the problem is made public.

## What counts as a security problem

akantu-rs is a numerical library. A problem is a security problem when input
that an attacker can control leads to more than a wrong result:

- memory unsafety: undefined behaviour reachable from safe Rust, or a defect
  in the `unsafe` code (the bindings to MPI, MUMPS, PETSc, Scotch and the C
  library, the Python bindings and their NumPy views, the C ABI); every
  `unsafe` site is listed in its crate's `UNSAFE.md`;
- the parsers and readers on untrusted input: the `.dat` input-file parser and
  its expressions, the Gmsh and Diana mesh readers, and the fixture and `.npy`
  readers of `akantu-validation`. A crafted file that crashes the process,
  exhausts memory or time without bound, or reads or writes outside its
  buffers is in scope. One known limit is not treated as a vulnerability: the input-file
  parser evaluates a referenced variable again at each reference, with no
  cache, as upstream does, so k definitions that each reference the previous
  one twice take 2^k evaluations (a 24-line chain takes seconds, a 40-line one
  days). Do not parse untrusted `.dat` files without a time limit;
- the CI workflows: a way to run code with write permissions or to read a
  token from a pull request.

Wrong numerical results, a refused configuration and differences from C++
Akantu are bugs: report them in the issue tracker (`CONTRIBUTING.md` says what
to include). Defects of the C++ library itself belong to the Akantu project
(https://gitlab.com/akantu/akantu).

## Supported versions

Fixes go into the `main` branch and the latest release.
