# Notices

akantu-rs is a Rust port of **Akantu** (https://gitlab.com/akantu/akantu),
Copyright (©) 2010-2023 EPFL (Ecole Polytechnique Fédérale de Lausanne),
Laboratory (LSMS - Laboratoire de Simulation en Mécanique des Solides).

Akantu is free software: you can redistribute it and/or modify it under the
terms of the GNU Lesser General Public License as published by the Free
Software Foundation, either version 3 of the License, or (at your option) any
later version.

As a derivative work, akantu-rs is distributed under the same terms:
**LGPL-3.0-or-later**. See `COPYING` (GNU GPL v3) and `COPYING.lesser`
(GNU LGPL v3).

Reference source used for the port: commit
`35a851780d0b9471d258ed53a003ae98aaa95ea0` (`v5.0.7-post1-554-g35a851780`).

The user manual (`docs/manual`) adapts parts of the Akantu manual
(`doc/dev-doc/manual`, © 2020-2023 EPFL/LSMS, LGPLv3); see
`docs/manual/NOTICE`.

## Third-party source code included in akantu-rs

### IOHelper 1.1.1 (LGPL-3.0-or-later)

The dumpers in `crates/akantu-io/src/dumper/` (`base.rs`, `base64.rs`,
`cfmt.rs`, `lammps.rs`, `paraview.rs`, `text.rs`, `vtk.rs`) are Rust ports of
**IOHelper** (`third-party/iohelper` of the reference, a separate EPFL
project): Copyright (©) 2010-2012, 2014 EPFL (Ecole Polytechnique Fédérale de
Lausanne), Laboratory (LSMS - Laboratoire de Simulation en Mécanique des
Solides); authors Guillaume Anciaux, Nicolas Richart, David Simon Kammer, Till
Junge. IOHelper is distributed under the GNU Lesser General Public License,
version 3 or later, the same terms as akantu-rs; each file carries the
notice.

### Eigen 3.4.0 (MPL-2.0)

The files `crates/akantu-linalg/src/dense/eigen/mod.rs`, `real_schur.rs`,
`self_adjoint.rs`, `crates/akantu-core/src/eigen_gebp.rs` and
`crates/akantu-linalg/src/eigen_sparse_lu/{mod,colamd,etree,kernels,dense}.rs`
are Rust translations of parts of **Eigen**
(https://eigen.tuxfamily.org, version 3.4.0) and remain under the
**Mozilla Public License 2.0** (`LICENSES/MPL-2.0.txt`); each file carries
the complete upstream notice of every Eigen header it derives from. Copyright
holders named in those notices:

- Copyright (C) 2006-2010 Benoit Jacob <jacob.benoit.1@gmail.com>
- Copyright (C) 2008-2016 Gael Guennebaud <gael.guennebaud@inria.fr>
- Copyright (C) 2010-2012 Jitse Niesen <jitse@maths.leeds.ac.uk>
- Copyright (C) 2014 Pedro Gonnet (pedro.gonnet@gmail.com)
- Copyright (c) 2021, NVIDIA CORPORATION. All rights reserved.
- Copyright (C) 2012 Désiré Nuentsa-Wakam <desire.nuentsa_wakam@inria.fr>
  (SparseLU, COLAMD and column elimination tree ports in Eigen)

(per-file years and holders as stated in each file). The source form of these
files is available in this repository; MPL-2.0 is file-level copyleft, and
the remainder of akantu-rs is distributed under LGPL-3.0-or-later as a
Larger Work (MPL-2.0 §3.3).

### SuperLU and COLAMD notices carried by Eigen's sparse LU (`eigen_sparse_lu/`)

Eigen's `SparseLU` module is a modified version of parts of **SuperLU**
(version 3.0–4.1, Univ. of California Berkeley, Xerox Palo Alto Research
Center, and Lawrence Berkeley National Lab), and Eigen's
`OrderingMethods/Eigen_Colamd.h` is modified from the **colamd/symamd**
library. The Rust translations in `crates/akantu-linalg/src/eigen_sparse_lu/`
reproduce these notices verbatim in their file headers and state that the
code was modified, as the notices require:

- Copyright (c) 1994 by Xerox Corporation. All rights reserved. (SuperLU:
  "THIS MATERIAL IS PROVIDED AS IS, WITH ABSOLUTELY NO WARRANTY EXPRESSED OR
  IMPLIED. ANY USE IS AT YOUR OWN RISK. Permission is hereby granted to use
  or copy this program for any purpose, provided the above notices are
  retained on all copies. Permission to modify the code and to distribute
  modified code is granted, provided the above notices are retained, and a
  notice that the code was modified is included with the above copyright
  notice.")
- COLAMD/SYMAMD version 2.3 (September 8, 2003), authors Stefan I. Larimore
  and Timothy A. Davis, University of Florida; algorithm developed in
  collaboration with John Gilbert (Xerox PARC) and Esmond Ng (Oak Ridge
  National Laboratory). Copyright (c) 1998-2003 by the University of
  Florida. All Rights Reserved. ("Permission is hereby granted to use, copy,
  modify, and/or distribute this program, provided that the Copyright, this
  License, and the Availability of the original version is retained on all
  copies and made accessible to the end-user of any code or package that
  includes COLAMD or any modified version of COLAMD." Availability: the
  colamd/symamd library is available at http://www.suitesparse.com.)

### GNU C Library (glibc) 2.41: `log`, `exp`, `pow`, `tan`, `sin`, `cos`, `sincos`

`crates/akantu-core/src/random/libm.rs` and
`crates/akantu-core/src/random/libm_tables.rs` are Rust translations (modified
to follow the instruction sequence of the oracle's compiled libm) of the
following files of the GNU C Library 2.41 (Debian `2.41-12+deb13u4`),
`sysdeps/ieee754/dbl-64/`:

* `e_log.c`, `e_exp.c`, `e_pow.c`, `math_config.h`, `math_err.c`,
  `e_exp_data.c`, `e_log_data.c`, `e_pow_log_data.c`:
  Copyright (C) 2018-2025 Free Software Foundation, Inc. This file is part of
  the GNU C Library. (Code contributed to glibc from the Arm Optimized
  Routines project.)
* `s_tan.c`: IBM Accurate Mathematical Library, written by International
  Business Machines Corp. Copyright (C) 2001-2025 Free Software Foundation,
  Inc.
* `dla.h`, `utan.h`, `utan.tbl`: IBM Accurate Mathematical Library.
  Copyright (C) 2001-2025 Free Software Foundation, Inc.

`crates/akantu-core/src/glibc_libm/sincos.rs` and
`crates/akantu-core/src/glibc_libm/sincos_table.rs` are
Rust translations, in the same manner, of:

* `s_sin.c`: IBM Accurate Mathematical Library, written by International
  Business Machines Corp. Copyright (C) 2001-2025 Free Software Foundation,
  Inc.
* `usncs.h`, `sincostab.c`: IBM Accurate Mathematical Library. Copyright (C)
  2001-2025 Free Software Foundation, Inc.
* `s_sincos.c`: Copyright (C) 1997-2025 Free Software Foundation, Inc. This
  file is part of the GNU C Library.

The GNU C Library is free software; you can redistribute it and/or modify it
under the terms of the GNU Lesser General Public License as published by the
Free Software Foundation; either version 2.1 of the License, or (at your
option) any later version. akantu-rs uses these translations under the GNU
Lesser General Public License version 3 or later, as permitted by that
"any later version" clause. The upstream notices are reproduced in the file
headers (and regenerated by `oracle/probes/random/gen_libm_tables.py` and
`oracle/probes/random/gen_sincos_table.py`).

### CGAL 5.6.2: LGPL packages (in `akantu-geometry`)

`crates/akantu-geometry/src/{kernel.rs,intersections.rs,number/}` contain
Rust translations of code from **CGAL** (https://www.cgal.org) 5.6.2
packages Kernel_23, Cartesian_kernel, Intersections_3 and Number_types,
distributed by CGAL under `LGPL-3.0-or-later OR LicenseRef-Commercial` and
used here under the GNU Lesser General Public License, version 3 or later:

    Copyright (c) 1999-2016 Utrecht University (The Netherlands),
    ETH Zurich (Switzerland), INRIA Sophia-Antipolis (France),
    Max-Planck-Institute Saarbruecken (Germany), and Tel-Aviv University (Israel).
    Copyright (c) 2001-2009 INRIA Sophia-Antipolis (France).
    Copyright (c) 2006-2008 Max-Planck-Institute Saarbruecken (Germany).
    Copyright (c) 2009, 2010, 2012, 2014 GeometryFactory (France).
    Copyright (c) 2008 ETH Zurich (Switzerland).

The exact notices and authors are reproduced in each file header; the file
list is in `docs/DEPENDENCIES.md` ("Geometry").

### CGAL 5.6.2: GPL packages (only in the opt-in crate `akantu-geometry-cgal`)

`crates/akantu-geometry-cgal` contains Rust translations of the CGAL 5.6.2
packages **AABB_tree**, **Circular_kernel_3** and
**Algebraic_kernel_for_spheres**, distributed by CGAL under
`GPL-3.0-or-later OR LicenseRef-Commercial` and used here under the **GNU
General Public License, version 3 or later** (`COPYING`):

    Copyright (c) 2005-2011 INRIA Sophia-Antipolis (France).
    Copyright (c) 2008 ETH Zurich (Switzerland).
    Authors: Camille Wormser, Pierre Alliez, Stéphane Tayeb, Monique Teillaud,
    Sylvain Pion, Pedro Machado, Julien Hazebrouck, Damien Leroy.

**This crate, and every build that includes it** (umbrella Cargo feature
`cgal`, OFF by default; decision record D-042), **is distributed under
GPL-3.0-or-later as a whole.** The default akantu-rs build does not include
it and remains LGPL-3.0-or-later. See `crates/akantu-geometry-cgal/README.md`.
The umbrella feature `igfem` implies `cgal` (the IGFEM growing-gel mesh
generation uses the GPL sphere intersector): **a build that enables `cgal`
or `igfem` is GPL-3.0-or-later as a whole**, and so is a build of the crate
`akantu-igfem` with its own feature `cgal`, the switch that `igfem` turns on
(`crates/akantu-igfem/README.md`). These three features are the only
declared GPL switches and none of them is a default feature (guard
`crates/akantu-geometry/tests/license_separation.rs`).

### Behaviour reproduced without code: the C++ standard library

`crates/akantu-core/src/random/engine.rs` and `distributions_impl.rs`
reproduce the observable behaviour of the oracle's GCC libstdc++ random-number
engine and distributions. They contain no libstdc++ code: they are a
clean-room implementation from the C++ standard, published algorithms and
black-box experiments (decision D-030; provenance in
`crates/akantu-core/src/random/CLEANROOM_LOG.md`).

`crates/akantu-parser/src/stream.rs` (stream extraction and number
printing) likewise reproduces the observable behaviour of the oracle's
libstdc++/glibc stream conversions without containing their code
(clean-room, D-030; provenance in `crates/akantu-parser/CLEANROOM_LOG.md`).

`crates/akantu-geometry/src/cxx.rs` reproduces the observable behaviour of
libstdc++'s `std::nth_element` (used by CGAL's AABB tree build) and of
`std::set::insert` with upstream's invalid comparator `segmentPairsLess`
without containing libstdc++ code (clean-room, D-030; provenance in
`crates/akantu-geometry/CLEANROOM_LOG.md`).

`crates/akantu-linalg/src/dense/complex.rs` reproduces the observable
behaviour of the oracle's `std::complex<double>` division (libgcc
`__divdc3`) without containing libgcc code (clean-room, D-030; provenance in
`crates/akantu-linalg/src/dense/COMPLEX_CLEANROOM.md` and
`COMPLEX_CLEANROOM_LOG.md`).

The clean-room logs and contracts (`CLEANROOM_LOG.md`, `CLEANROOM_API.md`,
`COMPLEX_CLEANROOM*.md`) are kept next to the code they describe, in the
`src/` directories and crate roots named above, as the licensing evidence
for these files; `docs/DEPENDENCIES.md` ("Behaviour reproduced clean-room")
lists them.

### Boost.Spirit 1.83 (BSL-1.0)

`crates/akantu-parser/src/expr.rs` contains real-number parsing code derived
from **Boost.Spirit 1.83** (X3 `double_`, Qi `real_parser<double>`; files
listed in `docs/DEPENDENCIES.md`), distributed under the Boost Software
License 1.0 (`LICENSES/BSL-1.0.txt`):

    Copyright (c) 2001-2019 Joel de Guzman
    Copyright (c) 2001-2011 Hartmut Kaiser
    Copyright (c) 2011 Jan Frederick Eick
    Copyright (c) 2011 Christopher Jefferson
    Copyright (c) 2006 Stephen Nutt

`crates/akantu-parser/src/grammar.rs` reproduces the position counting of
Boost.Spirit Classic `position_iterator` (Boost Software License 1.0):

    Copyright (c) 2002 Juan Carlos Arevalo-Baeza
    Copyright (c) 2002-2006 Hartmut Kaiser
    Copyright (c) 2003 Giovanni Bajo

Other third-party components and native dependencies are tracked in
`docs/DEPENDENCIES.md`.

## Benchmark data copied from the Akantu team's performance tests

`benchmarks/cohesive-perf/` contains files copied from
https://gitlab.com/akantu/performance-testing-cohesive at commit
`483bace8a03850ae49ee2feb68636d2f64eae3ee`: the four CSV files of
`results/published-jed/`, the geometry `inputs/cube.geo` (one line changed)
and the input files `inputs/material*.dat`. That repository carries no
licence file, and these files carry no licence notice; no licence is
asserted for them here. They are reproduced, with this attribution, as
published reference measurements and inputs of the Akantu team, and will be
removed at the authors' request. The driver `src/cohesive_extrinsic.cc` from
the same repository keeps its own LGPL-3.0-or-later header.

## Optional native libraries linked dynamically (not included)

### MUMPS 5.7.3 (CeCILL-C), feature `akantu-linalg/mumps`

Builds with the `mumps` feature link the system's parallel MUMPS library
(`libdmumps`, `libmumps_common`; "Copyright 1991-2024 CERFACS, CNRS, ENS
Lyon, INP Toulouse, Inria, Mumps Technologies, University of Bordeaux",
released under the CeCILL-C license, per the header of `dmumps_c.h` 5.7.3)
and, through it, ScaLAPACK (BSD-3), PT-Scotch/Scotch (CeCILL-C),
METIS (Apache-2.0), BLAS/LAPACK (OpenBLAS, BSD-3) and Open MPI (BSD-3). No
source code of these libraries is included in akantu-rs; the C shim
`crates/akantu-linalg/src/mumps/shim.c` is akantu-rs code that uses MUMPS's
public C interface (`dmumps_c.h`). Redistributors of such binaries must
ship the notices of the linked libraries (`docs/DEPENDENCIES.md`, section
"MUMPS").

### PETSc 3.22.5 (BSD-2-Clause), features `akantu-linalg/petsc`, `akantu-solver/petsc`, `akantu-phase-field/petsc`, `akantu-solid/petsc`, `akantu-couplers/petsc`

Builds with the `petsc` feature link the system's PETSc library
(`libpetsc_real`; "Copyright (c) 1991-2024, UChicago Argonne, LLC and the
PETSc Development Team", BSD-2-Clause, Debian `libpetsc-real3.22` in
`docker/petsc`) and, through it, the libraries Debian's PETSc is built with
(MUMPS and ScaLAPACK, hypre, SuperLU/SuperLU_DIST, SuiteSparse, PT-Scotch,
FFTW, HDF5, OpenBLAS, Open MPI; licenses in `docs/DEPENDENCIES.md`). No
source code of these libraries is included in akantu-rs; the C shim
`crates/akantu-linalg/src/petsc/shim.c` is akantu-rs code (ported from
Akantu's PETSc classes, LGPL-3.0-or-later) that uses PETSc's public C
interface. Redistributors of such binaries must ship the notices of the
linked libraries.

**GPL components.** Debian's `libpetsc_real.so.3.22`
(the oracle's and the test image's, byte-identical) has run-time
dependencies (`DT_NEEDED`, checked with `ldd` in `akantu-rs-petsc:dev`) on
libraries that contain **GPL-2.0-or-later** code, per Debian's copyright
files: SuiteSparse 7.10.1 `libumfpack.so.6` (`UMFPACK/*`: GPL-2+),
`libspqr.so.4` (`SPQR/*`: GPL-2+) and `libcholmod.so.5` (the
`CHOLMOD/{Supernodal, MatrixOps, Modify, GPU}/*` modules: GPL-2+; the
library exports Supernodal and Modify symbols, so GPL modules are compiled
in), and FFTW 3.3.10 `libfftw3.so.3` /
`libfftw3_mpi.so.3` (GPL-2+). Through SuperLU_DIST, `libCombBLAS.so.2.0.0`
is linked too; Debian lists its header-only `psort` parallel sort
(`psort-1.0/include/psort/{funnel,sort}.*`) as GPL-2+, and whether that code
is instantiated inside the library was not determined (no `psort` symbol is
exported). akantu-rs never calls them (no UMFPACK,
CHOLMOD, SPQR or FFTW solver type is used; upstream Akantu neither), but a
binary built with the `petsc` feature against this PETSc is dynamically
linked with them. Under the usual (FSF) reading such a program is a
combined work: **a binary built with the `petsc` feature against Debian's
PETSc may be distributed only under GPL-compatible terms, in practice
under GPL-3.0-or-later as a whole** (akantu-rs's LGPL-3.0-or-later code and
the GPL-2.0-or-later libraries are both compatible with GPL-3.0-or-later),
the same situation as a build with the `cgal` feature (D-042). The
akantu-rs **source** stays LGPL-3.0-or-later (the shim uses only PETSc's
BSD-licensed API), and the **default build** (feature `petsc` off) links no
PETSc and no GPL code. An LGPL-compatible distribution of a binary with
PETSc needs a PETSc configured without those components (e.g.
`--with-suitesparse=0 --with-fftw=0`, and without SuperLU_DIST / CombBLAS
unless the `psort` question is settled; akantu-rs needs none of them).
**Decision D-076:** the `petsc` feature stays off by
default and no reduced PETSc build is made; a binary built with it against
Debian's PETSc is distributed under GPL-3.0-or-later as a whole, as for
`cgal` (analysis: `docs/DEPENDENCIES.md`).
