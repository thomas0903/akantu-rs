// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/solver/sparse_matrix_petsc.cc
// (SparseMatrixPETSc: constructor sequence, addValues/applyModifications,
// clearProfile, saveMatrix, matVecMul), src/solver/solver_vector_petsc.cc
// (SolverVectorPETSc), src/solver/sparse_solver_petsc.cc
// (SparseSolverPETSc: KSPCreate, setOperators, solve),
// src/io/parser/parsable_petsc.hh (private PetscOptions database),
// src/model/common/dof_manager/dof_manager_petsc.cc:205-260 (AO from the
// owned global equations), src/model/common/non_linear_solver/
// {non_linear_solver_petsc.cc, non_linear_solver_tao.cc} (SNES/TAO
// objects and solves).

//! RAII wrappers of PETSc objects. Collective methods take the
//! [`NativeComm`] capability of an agreed native entry — bound to that
//! entry's closure and checked to be the object's own communicator
//! — and wait for the PETSc lock inside it
//! ([`PetscLock::enter_native`], lock order MPI → PETSc); local
//! methods only try the PETSc lock (re-entrant inside a native entry).
//! Dropping an object queues its collective destroy through
//! `Communicator::release_native`.

use std::ffi::{c_char, c_int, c_void};

use akantu_comm::{Communicator, NativeComm};
use akantu_core::{Error, Result};

use super::callbacks::{self, NativeCallbacks, UserData};
use super::ffi::{self, PetscInt};
use super::session::{PetscLock, Session, c_string, check};

/// PETSc object kinds (for destruction).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Mat,
    Vec,
    Ksp,
    Snes,
    Tao,
    Options,
}

/// An owned PETSc object handle.
#[derive(Clone, Copy, Debug)]
struct Ptr(*mut c_void);

// SAFETY: a PETSc object is not thread-affine memory; every use of the
// handle is serialised by the process-wide PETSc lock and, for MPI
// communication, by the MPI thread policy (native entries); the handle is
// moved into exactly one destruction closure.
unsafe impl Send for Ptr {}

fn destroy(kind: Kind, p: Ptr) -> c_int {
    // SAFETY: `p` is the unique owned handle of an object of kind `kind`
    // created by the matching shim constructor (or a reference taken with
    // `akantu_petsc_mat_reference`), destroyed exactly once.
    unsafe {
        match kind {
            Kind::Mat => ffi::akantu_petsc_mat_destroy(p.0),
            Kind::Vec => ffi::akantu_petsc_vec_destroy(p.0),
            Kind::Ksp => ffi::akantu_petsc_ksp_destroy(p.0),
            Kind::Snes => ffi::akantu_petsc_snes_destroy(p.0),
            Kind::Tao => ffi::akantu_petsc_tao_destroy(p.0),
            Kind::Options => ffi::akantu_petsc_options_destroy(p.0),
        }
    }
}

/// An owned PETSc object: handle, kind, communicator and session (both
/// kept alive until the object is destroyed).
#[derive(Debug)]
struct Native {
    ptr: Ptr,
    kind: Kind,
    comm: Communicator,
    session: Session,
}

// SAFETY: see `Ptr`; the communicator and session handles are `Send`.
unsafe impl Send for Native {}

impl Drop for Native {
    fn drop(&mut self) {
        if self.ptr.0.is_null() {
            return;
        }
        let (kind, ptr) = (self.kind, self.ptr);
        let session = self.session.clone();
        // Collective for parallel objects (`MatDestroy`, `KSPDestroy`, …):
        // run under the MPI thread policy before MPI is finalised, in SPMD
        // order (every rank drops its object at the same point).
        let queued = self.comm.release_native(move || {
            let _g = PetscLock::enter_waiting();
            let rc = destroy(kind, ptr);
            if rc != 0 {
                log::error!("PETSc {kind:?} destruction failed (code {rc})");
            }
            // The session (and with it `PetscFinalize`) outlives the
            // object.
            drop(session);
        });
        if !queued {
            log::error!("PETSc {kind:?} on a non-MPI communicator: leaked");
        }
    }
}

/// The capability of an agreed native entry must be the one of `comm`, the
/// communicator a PETSc object lives on (a capability of
/// another communicator — a duplicate, a split — would run the object's
/// collectives outside its own agreed entry). A mismatch is a programming
/// error, identical on every rank in an SPMD program.
fn check_capability(nc: &NativeComm<'_>, comm: &Communicator, what: &str) -> Result<()> {
    if nc.is_for(comm) {
        Ok(())
    } else {
        Err(Error::invalid(format!(
            "{what}: the native capability was issued for another communicator than the PETSc \
             object's"
        )))
    }
}

/// Every vector has the matrix's layout: the same owned rows and the same
/// global size (a solve's work vectors are duplicated
/// from these, and the callbacks slice PETSc's arrays with the owned count).
fn check_layout(a: &Matrix, vectors: &[&Vector], context: &'static str) -> Result<()> {
    for v in vectors {
        if v.n_owned != a.n_owned {
            return Err(Error::DimensionMismatch {
                context,
                expected: a.n_owned,
                got: v.n_owned,
            });
        }
        if v.size != a.size {
            return Err(Error::DimensionMismatch {
                context,
                expected: a.size,
                got: v.size,
            });
        }
    }
    Ok(())
}

impl Native {
    fn new(ptr: *mut c_void, kind: Kind, comm: &Communicator, session: &Session) -> Result<Self> {
        if ptr.is_null() {
            return Err(Error::Backend {
                backend: "petsc",
                code: -1,
                message: format!("PETSc returned a null {kind:?}"),
            });
        }
        Ok(Self {
            ptr: Ptr(ptr),
            kind,
            comm: comm.clone(),
            session: session.clone(),
        })
    }
}

fn len_i64(n: usize, what: &str) -> Result<i64> {
    i64::try_from(n).map_err(|_| Error::invalid(format!("{what}: length {n} exceeds 64 bits")))
}

/// A PETSc index `k` of a dimension of global length `len`: `0 ≤ k < len`.
/// PETSc itself skips negative indices and leaves
/// some out-of-range ones unchecked in its non-debug builds (the hash
/// insertion of an unpreallocated `MATSEQAIJ` writes out of bounds for a row
/// past the end), so every index a safe wrapper hands to PETSc is
/// checked here first.
fn check_index(k: PetscInt, len: usize, what: &'static str) -> Result<()> {
    match usize::try_from(k) {
        Ok(u) if u < len => Ok(()),
        Ok(u) => Err(Error::OutOfRange {
            what,
            index: u,
            len,
        }),
        Err(_) => Err(Error::invalid(format!(
            "{what}: negative PETSc index {k} (the matrix has {len} rows / columns)"
        ))),
    }
}

/// Local and global sizes of a new distributed object: the owned rows must
/// fit the global size (the cross-rank sum is checked by PETSc's
/// `PetscLayoutSetUp`, always on).
fn check_sizes(n_owned: usize, size: usize, what: &str) -> Result<()> {
    if n_owned > size {
        return Err(Error::invalid(format!(
            "{what}: {n_owned} owned rows exceed the global size {size}"
        )));
    }
    Ok(())
}

fn read_c(buf: &[c_char]) -> String {
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| u8::from_ne_bytes(c.to_ne_bytes()))
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

// --------------------------------------------------------------- options

/// A private PETSc options database (upstream `ParsablePETSc::options`):
/// `-name value` pairs set from parser sections and `set` calls, pushed
/// around `XSetFromOptions`.
#[derive(Debug)]
pub struct Options {
    native: Native,
}

impl Options {
    /// New empty database (local).
    pub fn new(comm: &Communicator, session: &Session) -> Result<Self> {
        let _g = PetscLock::try_enter()?;
        let mut p = std::ptr::null_mut();
        // SAFETY: one valid out-pointer; PetscOptionsCreate is local.
        check(
            unsafe { ffi::akantu_petsc_options_create(&raw mut p) },
            "PetscOptionsCreate",
        )?;
        Ok(Self {
            native: Native::new(p, Kind::Options, comm, session)?,
        })
    }

    /// `PetscOptionsSetValue(options, "-<name>", value)` (local).
    pub fn set(&mut self, name: &str, value: &str) -> Result<()> {
        let _g = PetscLock::try_enter()?;
        let n = c_string(&format!("-{name}"), "PETSc option name")?;
        let v = c_string(value, "PETSc option value")?;
        // SAFETY: live database handle; NUL-terminated name and value valid
        // for the call (PETSc copies them).
        check(
            unsafe { ffi::akantu_petsc_options_set(self.native.ptr.0, n.as_ptr(), v.as_ptr()) },
            "PetscOptionsSetValue",
        )
    }

    fn ptr(&self) -> *mut c_void {
        self.native.ptr.0
    }
}

// ------------------------------------------------- application ordering

/// Map the akantu global equations `idx` (negative entries untouched) to
/// PETSc's natural ordering defined by `owned_app` — the akantu global
/// equations owned by this rank, in local order (upstream
/// `DOFManagerPETSc::setISLocalToGlobalMapping` + `AOApplicationToPetsc`;
/// memory-scalable AO). Collective.
pub fn ao_map(nc: &NativeComm<'_>, owned_app: &[PetscInt], idx: &mut [PetscInt]) -> Result<()> {
    let _g = PetscLock::enter_native(nc);
    let no = len_i64(owned_app.len(), "AO")?;
    let n = len_i64(idx.len(), "AO")?;
    // SAFETY: live communicator of the agreed native entry; `owned_app`
    // has `no` readable PetscInts, `idx` `n` writable ones, both valid for
    // the call (the AO is created and destroyed inside it).
    check(
        unsafe { ffi::akantu_petsc_ao_map(nc.raw(), no, owned_app.as_ptr(), n, idx.as_mut_ptr()) },
        "AOApplicationToPetsc",
    )
}

// -------------------------------------------------------------- matrices

/// A distributed PETSc matrix (upstream `SparseMatrixPETSc`): `n_owned`
/// rows (and columns) of this rank out of `size`.
#[derive(Debug)]
pub struct Matrix {
    native: Native,
    n_owned: usize,
    size: usize,
}

impl Matrix {
    /// Upstream constructor sequence (`MatCreate`, name, `MatSetFromOptions`,
    /// sizes, `MatSetUp`, row orientation, new nonzeros allowed,
    /// `MAT_SYMMETRIC` when `symmetric`). Collective.
    pub fn create(
        nc: &NativeComm<'_>,
        comm: &Communicator,
        session: &Session,
        name: &str,
        n_owned: usize,
        size: usize,
        symmetric: bool,
    ) -> Result<Self> {
        let _g = PetscLock::enter_native(nc);
        check_capability(nc, comm, "create")?;
        check_sizes(n_owned, size, "petsc::Matrix::create")?;
        let cname = c_string(name, "PETSc matrix name")?;
        let mut p = std::ptr::null_mut();
        // SAFETY: live communicator of the agreed native entry; valid name
        // and out-pointer for the call.
        check(
            unsafe {
                ffi::akantu_petsc_mat_create(
                    nc.raw(),
                    cname.as_ptr(),
                    len_i64(n_owned, "matrix rows")?,
                    len_i64(size, "matrix order")?,
                    c_int::from(symmetric),
                    &raw mut p,
                )
            },
            "MatCreate",
        )?;
        Ok(Self {
            native: Native::new(p, Kind::Mat, comm, session)?,
            n_owned,
            size,
        })
    }

    /// Rows owned by this rank.
    #[must_use]
    pub fn n_owned(&self) -> usize {
        self.n_owned
    }

    /// Global order.
    #[must_use]
    pub fn size(&self) -> usize {
        self.size
    }

    /// `MAT_SYMMETRIC = MAT_STRUCTURALLY_SYMMETRIC = false` (upstream
    /// `addValues` with an unsymmetric block). Logically collective.
    pub fn set_unsymmetric(&mut self, nc: &NativeComm<'_>) -> Result<()> {
        let _g = PetscLock::enter_native(nc);
        check_capability(nc, &self.native.comm, "set_unsymmetric")?;
        // SAFETY: live matrix handle.
        check(
            unsafe { ffi::akantu_petsc_mat_set_unsymmetric(self.native.ptr.0) },
            "MatSetOption(MAT_SYMMETRIC)",
        )
    }

    /// `MatZeroEntries`.
    pub fn zero_entries(&mut self, nc: &NativeComm<'_>) -> Result<()> {
        let _g = PetscLock::enter_native(nc);
        check_capability(nc, &self.native.comm, "zero_entries")?;
        // SAFETY: live matrix handle.
        check(
            unsafe { ffi::akantu_petsc_mat_zero_entries(self.native.ptr.0) },
            "MatZeroEntries",
        )
    }

    /// `MatResetPreallocation` (upstream `clearProfile`): the next assembly
    /// may use another nonzero pattern.
    pub fn reset_preallocation(&mut self, nc: &NativeComm<'_>) -> Result<()> {
        let _g = PetscLock::enter_native(nc);
        check_capability(nc, &self.native.comm, "reset_preallocation")?;
        // SAFETY: live matrix handle.
        check(
            unsafe { ffi::akantu_petsc_mat_reset_preallocation(self.native.ptr.0) },
            "MatResetPreallocation",
        )
    }

    /// Add `vals` at `(row, cols)` (PETSc indices, `ADD_VALUES`; rows of
    /// other ranks are stashed until [`Matrix::assemble`]). Local. `row` and
    /// every column must lie in `0..size` (an `Error::OutOfRange`, or
    /// `Error::InvalidArgument` for a negative index, before any PETSc call).
    pub fn add_row(&mut self, row: PetscInt, cols: &[PetscInt], vals: &[f64]) -> Result<()> {
        if cols.len() != vals.len() {
            return Err(Error::DimensionMismatch {
                context: "petsc::Matrix::add_row",
                expected: cols.len(),
                got: vals.len(),
            });
        }
        check_index(row, self.size, "petsc::Matrix::add_row row")?;
        for &c in cols {
            check_index(c, self.size, "petsc::Matrix::add_row column")?;
        }
        let _g = PetscLock::try_enter()?;
        // SAFETY: live matrix handle; `cols`/`vals` hold `cols.len()`
        // readable entries for the call (PETSc copies them); `row` and every
        // column lie in `0..size` (checked above), so PETSc's unchecked
        // hash insertion of an unpreallocated matrix stays in bounds.
        check(
            unsafe {
                ffi::akantu_petsc_mat_add_row(
                    self.native.ptr.0,
                    row,
                    len_i64(cols.len(), "row")?,
                    cols.as_ptr(),
                    vals.as_ptr(),
                )
            },
            "MatSetValues",
        )
    }

    /// `MatAssemblyBegin/End(FINAL)` (upstream `applyModifications`).
    /// Collective.
    pub fn assemble(&mut self, nc: &NativeComm<'_>) -> Result<()> {
        let _g = PetscLock::enter_native(nc);
        check_capability(nc, &self.native.comm, "assemble")?;
        // SAFETY: live matrix handle; every rank of the matrix's
        // communicator is in the same agreed native entry.
        check(
            unsafe { ffi::akantu_petsc_mat_assemble(self.native.ptr.0) },
            "MatAssembly",
        )
    }

    /// Owned row range `[lo, hi)` (local).
    pub fn ownership(&self) -> Result<(i64, i64)> {
        let _g = PetscLock::try_enter()?;
        let (mut lo, mut hi) = (0, 0);
        // SAFETY: live matrix handle; two valid out-pointers.
        check(
            unsafe { ffi::akantu_petsc_mat_ownership(self.native.ptr.0, &raw mut lo, &raw mut hi) },
            "MatGetOwnershipRange",
        )?;
        Ok((lo, hi))
    }

    /// Values of owned `rows` × `cols` (row-major; tests and diagnostics).
    /// Local; every index must lie in `0..size` (checked here), rows
    /// must be owned (refused by PETSc, `MatGetValues_MPIAIJ`, always on).
    pub fn get_values(&self, rows: &[PetscInt], cols: &[PetscInt]) -> Result<Vec<f64>> {
        for &r in rows {
            check_index(r, self.size, "petsc::Matrix::get_values row")?;
        }
        for &c in cols {
            check_index(c, self.size, "petsc::Matrix::get_values column")?;
        }
        let count = rows.len().checked_mul(cols.len()).ok_or_else(|| {
            Error::invalid("petsc::Matrix::get_values: rows × columns overflow".to_string())
        })?;
        let _g = PetscLock::try_enter()?;
        let mut v = vec![0.0; count];
        // SAFETY: live, assembled matrix; `rows`/`cols` readable and in
        // range (checked above), `v` holds rows × cols writable doubles.
        check(
            unsafe {
                ffi::akantu_petsc_mat_get_values(
                    self.native.ptr.0,
                    len_i64(rows.len(), "rows")?,
                    rows.as_ptr(),
                    len_i64(cols.len(), "cols")?,
                    cols.as_ptr(),
                    v.as_mut_ptr(),
                )
            },
            "MatGetValues",
        )?;
        Ok(v)
    }

    /// Number of stored entries of the whole matrix (`MatGetInfo`,
    /// `MAT_GLOBAL_SUM`). Collective.
    pub fn nonzeros(&self, nc: &NativeComm<'_>) -> Result<f64> {
        let _g = PetscLock::enter_native(nc);
        check_capability(nc, &self.native.comm, "nonzeros")?;
        let mut nz = 0.0;
        // SAFETY: live matrix handle; one valid out-pointer; collective
        // inside the agreed native entry.
        check(
            unsafe { ffi::akantu_petsc_mat_nonzeros(self.native.ptr.0, &raw mut nz) },
            "MatGetInfo",
        )?;
        Ok(nz)
    }

    /// `y = A x` over the owned rows (PETSc `MatMult`). Collective.
    pub fn mult(&self, nc: &NativeComm<'_>, x: &[f64], y: &mut [f64]) -> Result<()> {
        if x.len() != self.n_owned || y.len() != self.n_owned {
            return Err(Error::DimensionMismatch {
                context: "petsc::Matrix::mult",
                expected: self.n_owned,
                got: x.len().min(y.len()),
            });
        }
        let _g = PetscLock::enter_native(nc);
        check_capability(nc, &self.native.comm, "mult")?;
        // SAFETY: live assembled matrix; `x`/`y` hold `n_owned` entries =
        // the matrix's local row/column count (the shim's vectors are
        // created from the matrix layout).
        check(
            unsafe {
                ffi::akantu_petsc_mat_mult(
                    self.native.ptr.0,
                    len_i64(self.n_owned, "owned rows")?,
                    x.as_ptr(),
                    y.as_mut_ptr(),
                )
            },
            "MatMult",
        )
    }

    /// Upstream `saveMatrix`: Matrix Market through a PETSc viewer.
    /// Collective.
    pub fn save_matrix_market(&self, nc: &NativeComm<'_>, path: &str) -> Result<()> {
        let _g = PetscLock::enter_native(nc);
        check_capability(nc, &self.native.comm, "save_matrix_market")?;
        let p = c_string(path, "file name")?;
        // SAFETY: live matrix; NUL-terminated path valid for the call.
        check(
            unsafe { ffi::akantu_petsc_mat_save_matrix_market(self.native.ptr.0, p.as_ptr()) },
            "MatView",
        )
    }

    /// A new reference to the same PETSc matrix (`PetscObjectReference`):
    /// keeps the matrix alive for a native solver that holds it.
    pub fn new_reference(&self) -> Result<Self> {
        let _g = PetscLock::try_enter()?;
        // SAFETY: live matrix handle; the extra reference is released by
        // the returned value's destruction.
        check(
            unsafe { ffi::akantu_petsc_mat_reference(self.native.ptr.0) },
            "PetscObjectReference",
        )?;
        Ok(Self {
            native: Native::new(
                self.native.ptr.0,
                Kind::Mat,
                &self.native.comm,
                &self.native.session,
            )?,
            n_owned: self.n_owned,
            size: self.size,
        })
    }

    fn ptr(&self) -> *mut c_void {
        self.native.ptr.0
    }
}

// --------------------------------------------------------------- vectors

/// A distributed PETSc vector (upstream `SolverVectorPETSc`, without the
/// ghost entries: akantu's local-equation vectors stay in Rust).
#[derive(Debug)]
pub struct Vector {
    native: Native,
    n_owned: usize,
    size: usize,
}

impl Vector {
    /// `VecCreate`, name, sizes `(n_owned, size)`, `VecSetFromOptions`.
    /// Collective.
    pub fn create(
        nc: &NativeComm<'_>,
        comm: &Communicator,
        session: &Session,
        name: &str,
        n_owned: usize,
        size: usize,
    ) -> Result<Self> {
        let _g = PetscLock::enter_native(nc);
        check_capability(nc, comm, "create")?;
        check_sizes(n_owned, size, "petsc::Vector::create")?;
        let cname = c_string(name, "PETSc vector name")?;
        let mut p = std::ptr::null_mut();
        // SAFETY: live communicator of the agreed native entry; valid name
        // and out-pointer.
        check(
            unsafe {
                ffi::akantu_petsc_vec_create(
                    nc.raw(),
                    cname.as_ptr(),
                    len_i64(n_owned, "vector rows")?,
                    len_i64(size, "vector size")?,
                    &raw mut p,
                )
            },
            "VecCreate",
        )?;
        Ok(Self {
            native: Native::new(p, Kind::Vec, comm, session)?,
            n_owned,
            size,
        })
    }

    /// Owned rows.
    #[must_use]
    pub fn n_owned(&self) -> usize {
        self.n_owned
    }

    /// Global size.
    #[must_use]
    pub fn size(&self) -> usize {
        self.size
    }

    /// Copy `vals` into the owned rows (local).
    pub fn set_owned(&mut self, vals: &[f64]) -> Result<()> {
        if vals.len() != self.n_owned {
            return Err(Error::DimensionMismatch {
                context: "petsc::Vector::set_owned",
                expected: self.n_owned,
                got: vals.len(),
            });
        }
        let _g = PetscLock::try_enter()?;
        // SAFETY: live vector whose local size is checked by the shim
        // against `vals.len()`; `vals` readable for the call.
        check(
            unsafe {
                ffi::akantu_petsc_vec_set_owned(
                    self.native.ptr.0,
                    len_i64(vals.len(), "vector")?,
                    vals.as_ptr(),
                )
            },
            "VecGetArray",
        )
    }

    /// Copy the owned rows into `vals` (local).
    pub fn get_owned(&self, vals: &mut [f64]) -> Result<()> {
        if vals.len() != self.n_owned {
            return Err(Error::DimensionMismatch {
                context: "petsc::Vector::get_owned",
                expected: self.n_owned,
                got: vals.len(),
            });
        }
        let _g = PetscLock::try_enter()?;
        // SAFETY: live vector (local size checked by the shim); `vals`
        // writable for the call.
        check(
            unsafe {
                ffi::akantu_petsc_vec_get_owned(
                    self.native.ptr.0,
                    len_i64(vals.len(), "vector")?,
                    vals.as_mut_ptr(),
                )
            },
            "VecGetArrayRead",
        )
    }

    /// `VecDot(self, other)` (BLAS `ddot` on the owned rows, then PETSc's
    /// reduction over the communicator; upstream's TAO objective uses it).
    /// Collective: inside an agreed native entry or region.
    pub fn dot(&self, nc: &NativeComm<'_>, other: &Vector) -> Result<f64> {
        let _g = PetscLock::enter_native(nc);
        check_capability(nc, &self.native.comm, "dot")?;
        check_capability(nc, &other.native.comm, "dot operand")?;
        if other.n_owned != self.n_owned || other.size != self.size {
            return Err(Error::DimensionMismatch {
                context: "petsc::Vector::dot",
                expected: self.n_owned,
                got: other.n_owned,
            });
        }
        let mut out = 0.0;
        // SAFETY: two live vectors of the same communicator (checked
        // against the native capability) and layout (checked above), every
        // rank of which is in the same agreed native entry; one valid
        // out-pointer; under the PETSc lock.
        check(
            unsafe {
                ffi::akantu_petsc_vec_dot(self.native.ptr.0, other.native.ptr.0, &raw mut out)
            },
            "VecDot",
        )?;
        Ok(out)
    }

    fn ptr(&self) -> *mut c_void {
        self.native.ptr.0
    }
}

// ------------------------------------------------------------------- KSP

/// Result of a KSP solve.
#[derive(Clone, Debug, PartialEq)]
pub struct KspOutcome {
    /// `KSPConvergedReason` (`> 0` converged, `< 0` diverged).
    pub reason: i32,
    /// `KSPConvergedReason` name.
    pub reason_string: String,
    /// Iterations.
    pub iterations: usize,
    /// Final residual norm (`KSPGetResidualNorm`).
    pub residual_norm: f64,
}

/// A PETSc linear solver (upstream `SparseSolverPETSc`).
#[derive(Debug)]
pub struct Ksp {
    native: Native,
}

impl Ksp {
    /// `KSPCreate` + name (upstream: no `KSPSetFromOptions` until options
    /// are set). Collective.
    pub fn create(
        nc: &NativeComm<'_>,
        comm: &Communicator,
        session: &Session,
        name: &str,
    ) -> Result<Self> {
        let _g = PetscLock::enter_native(nc);
        check_capability(nc, comm, "create")?;
        let cname = c_string(name, "PETSc KSP name")?;
        let mut p = std::ptr::null_mut();
        // SAFETY: live communicator of the agreed native entry; valid name
        // and out-pointer.
        check(
            unsafe { ffi::akantu_petsc_ksp_create(nc.raw(), cname.as_ptr(), &raw mut p) },
            "KSPCreate",
        )?;
        Ok(Self {
            native: Native::new(p, Kind::Ksp, comm, session)?,
        })
    }

    /// Upstream `ParsablePETSc::updateInternalParameters`: the private
    /// database pushed around `KSPSetFromOptions`, then the global database
    /// (command line, `PETSC_OPTIONS`). Collective.
    pub fn set_from_options(
        &mut self,
        nc: &NativeComm<'_>,
        options: Option<&Options>,
    ) -> Result<()> {
        let _g = PetscLock::enter_native(nc);
        check_capability(nc, &self.native.comm, "set_from_options")?;
        let o = options.map_or(std::ptr::null_mut(), Options::ptr);
        // SAFETY: live KSP; `o` null or a live options database.
        check(
            unsafe { ffi::akantu_petsc_ksp_set_from_options(self.native.ptr.0, o) },
            "KSPSetFromOptions",
        )
    }

    /// `KSPSetOperators(A, A)`, `KSPSetUp`, `KSPSolve(b, x)` with a zero
    /// initial guess (upstream `SparseSolverPETSc::solve`). Collective.
    pub fn solve(
        &mut self,
        nc: &NativeComm<'_>,
        a: &Matrix,
        b: &Vector,
        x: &mut Vector,
    ) -> Result<KspOutcome> {
        let _g = PetscLock::enter_native(nc);
        check_capability(nc, &self.native.comm, "solve")?;
        for comm in [&a.native.comm, &b.native.comm, &x.native.comm] {
            check_capability(nc, comm, "Ksp::solve operands")?;
        }
        check_layout(a, &[b, x], "petsc::Ksp::solve")?;
        let (mut setup_failed, mut reason, mut its, mut rnorm) = (0, 0, 0_i64, 0.0);
        // SAFETY: live KSP, matrix and vectors of the same communicator
        // (capabilities checked) and layout (local and global sizes checked
        // above); four valid out-pointers.
        let rc = unsafe {
            ffi::akantu_petsc_ksp_solve(
                self.native.ptr.0,
                a.ptr(),
                b.ptr(),
                x.ptr(),
                &raw mut setup_failed,
                &raw mut reason,
                &raw mut its,
                &raw mut rnorm,
            )
        };
        if setup_failed != 0 {
            // Upstream SparseSolverPETSc::setOperators' message.
            let detail = check(rc, "KSPSetUp")
                .err()
                .map_or_else(|| "no PETSc error code".to_owned(), |e| e.to_string());
            // Upstream's text (`sparse_solver_petsc.cc:72-78`),
            // the PETSc error appended after a newline.
            return Err(Error::LinearSolver(format!(
                "PETSc KSP sparse solver initialization failed: is your matrix singular ?\n{detail}"
            )));
        }
        check(rc, "KSPSolve")?;
        let mut buf = [0 as c_char; 128];
        // SAFETY: live KSP; `buf` has `buf.len()` writable characters.
        check(
            unsafe {
                ffi::akantu_petsc_ksp_reason_string(self.native.ptr.0, buf.as_mut_ptr(), buf.len())
            },
            "KSPGetConvergedReasonString",
        )?;
        Ok(KspOutcome {
            reason,
            reason_string: read_c(&buf),
            iterations: usize::try_from(its).unwrap_or(0),
            residual_norm: rnorm,
        })
    }

    /// `KSPReset`: operators, preconditioner and work vectors dropped (type
    /// and options kept), so that the next solve may use a matrix and
    /// vectors with other local sizes. Collective.
    pub fn reset(&mut self, nc: &NativeComm<'_>) -> Result<()> {
        let _g = PetscLock::enter_native(nc);
        check_capability(nc, &self.native.comm, "reset")?;
        // SAFETY: live KSP; every rank of its communicator is in the same
        // agreed native entry (the decision to reset is reduced).
        check(
            unsafe { ffi::akantu_petsc_ksp_reset(self.native.ptr.0) },
            "KSPReset",
        )
    }

    /// `(rtol, atol, dtol, max_it)` (local).
    pub fn tolerances(&self) -> Result<(f64, f64, f64, i64)> {
        let _g = PetscLock::try_enter()?;
        let (mut r, mut a, mut d, mut m) = (0.0, 0.0, 0.0, 0);
        // SAFETY: live KSP; four valid out-pointers.
        check(
            unsafe {
                ffi::akantu_petsc_ksp_tolerances(
                    self.native.ptr.0,
                    &raw mut r,
                    &raw mut a,
                    &raw mut d,
                    &raw mut m,
                )
            },
            "KSPGetTolerances",
        )?;
        Ok((r, a, d, m))
    }

    /// `(KSP type, PC type)` names (local; the PC type is set at setup).
    pub fn types(&self) -> Result<(String, String)> {
        let _g = PetscLock::try_enter()?;
        let mut k = [0 as c_char; 64];
        let mut p = [0 as c_char; 64];
        // SAFETY: live KSP; two buffers of `k.len()` writable characters.
        check(
            unsafe {
                ffi::akantu_petsc_ksp_types(
                    self.native.ptr.0,
                    k.as_mut_ptr(),
                    p.as_mut_ptr(),
                    k.len(),
                )
            },
            "KSPGetType",
        )?;
        Ok((read_c(&k), read_c(&p)))
    }
}

// ------------------------------------------------------------------ SNES

/// Result of a SNES solve (upstream `NonLinearSolverPETSc` parameters).
#[derive(Clone, Debug, PartialEq)]
pub struct SnesOutcome {
    /// `n_iter`.
    pub iterations: usize,
    /// `SNESConvergedReason`.
    pub reason: i32,
    /// `convergence_reason`.
    pub reason_string: String,
    /// `function_norm`.
    pub function_norm: f64,
    /// `solution_norm`.
    pub solution_norm: f64,
    /// `update_norm`.
    pub update_norm: f64,
    /// Convergence history of the function norm (`SNESSetConvergenceHistory`).
    pub history: Vec<f64>,
    /// Tolerances `(atol, rtol, stol, divtol)`.
    pub tolerances: (f64, f64, f64, f64),
    /// `(max_it, max_function_evaluations)`.
    pub limits: (i64, i64),
}

impl SnesOutcome {
    /// Upstream `error = petsc_a[n_iter] / petsc_a[0]`.
    #[must_use]
    pub fn error(&self) -> f64 {
        match (self.history.get(self.iterations), self.history.first()) {
            (Some(&last), Some(&first)) => last / first,
            _ => f64::NAN,
        }
    }
}

/// A PETSc non-linear solver (upstream `NonLinearSolverPETSc`'s `SNES`).
#[derive(Debug)]
pub struct Snes {
    native: Native,
}

/// Upstream `petsc_na`: length of the convergence history.
const HISTORY: usize = 1000;

impl Snes {
    /// `SNESCreate`, name, `SNESNEWTONLS`, `SNESSetFromOptions` (global
    /// database, as upstream's constructor). Collective.
    pub fn create(
        nc: &NativeComm<'_>,
        comm: &Communicator,
        session: &Session,
        name: &str,
    ) -> Result<Self> {
        let _g = PetscLock::enter_native(nc);
        check_capability(nc, comm, "create")?;
        let cname = c_string(name, "PETSc SNES name")?;
        let mut p = std::ptr::null_mut();
        // SAFETY: live communicator of the agreed native entry; valid name
        // and out-pointer.
        check(
            unsafe { ffi::akantu_petsc_snes_create(nc.raw(), cname.as_ptr(), &raw mut p) },
            "SNESCreate",
        )?;
        Ok(Self {
            native: Native::new(p, Kind::Snes, comm, session)?,
        })
    }

    /// Private database pushed around `SNESSetFromOptions` (which also
    /// configures the inner KSP), then the global database. Collective.
    pub fn set_from_options(
        &mut self,
        nc: &NativeComm<'_>,
        options: Option<&Options>,
    ) -> Result<()> {
        let _g = PetscLock::enter_native(nc);
        check_capability(nc, &self.native.comm, "set_from_options")?;
        let o = options.map_or(std::ptr::null_mut(), Options::ptr);
        // SAFETY: live SNES; `o` null or a live options database.
        check(
            unsafe { ffi::akantu_petsc_snes_set_from_options(self.native.ptr.0, o) },
            "SNESSetFromOptions",
        )
    }

    /// The native part of upstream `NonLinearSolverPETSc::solve`: function
    /// and Jacobian (`J` as matrix and preconditioner) routed to
    /// `callbacks`, history of 1000 entries, `x` and `r` zeroed,
    /// `SNESSolve(snes, NULL, x)`. Must run inside an agreed native region
    /// (the callbacks run collective akantu operations on this thread).
    /// An error returned while a callback failed is the callback's
    /// (agreed) failure; otherwise it is a PETSc failure. `aliased`
    /// reproduces upstream's vector aliasing (UD-428): every trial point is
    /// copied into `x` (SNES's iterate) and every function value into `r`.
    pub fn solve(
        &mut self,
        nc: &NativeComm<'_>,
        jacobian: &Matrix,
        x: &mut Vector,
        r: &mut Vector,
        callbacks: &mut dyn NativeCallbacks,
        aliased: bool,
    ) -> Result<SnesOutcome> {
        let _g = PetscLock::enter_native(nc);
        check_capability(nc, &self.native.comm, "solve")?;
        for comm in [&jacobian.native.comm, &x.native.comm, &r.native.comm] {
            check_capability(nc, comm, "Snes::solve operands")?;
        }
        let n = x.n_owned;
        check_layout(jacobian, &[x, r], "petsc::Snes::solve")?;
        let mut hist = vec![0.0; HISTORY];
        let mut hist_its: Vec<PetscInt> = vec![0; HISTORY];
        let mut rep = ffi::SnesReport::default();
        let mut user: UserData<'_> = callbacks;
        let user_ptr: *mut UserData<'_> = &raw mut user;
        // SAFETY: live SNES, matrix and vectors of one communicator and
        // layout; `hist`/`hist_its` hold HISTORY entries and outlive the
        // solve (SNES keeps the pointers only until the next solve resets
        // them); `user_ptr` points to `user` on this frame, alive for the
        // whole SNESSolve and used only by the trampolines, which run on
        // this thread during the call; `rep` is a valid out-pointer.
        let rc = unsafe {
            ffi::akantu_petsc_snes_solve(
                self.native.ptr.0,
                jacobian.ptr(),
                x.ptr(),
                r.ptr(),
                hist.as_mut_ptr(),
                hist_its.as_mut_ptr(),
                len_i64(HISTORY, "history")?,
                callbacks::residual,
                callbacks::jacobian,
                user_ptr.cast::<c_void>(),
                len_i64(n, "owned rows")?,
                c_int::from(aliased),
                &raw mut rep,
            )
        };
        check(rc, "SNESSolve")?;
        let hlen = usize::try_from(rep.history_len).unwrap_or(0).min(HISTORY);
        hist.truncate(hlen);
        Ok(SnesOutcome {
            iterations: usize::try_from(rep.iterations).unwrap_or(0),
            reason: rep.reason,
            reason_string: read_c(&rep.reason_string),
            function_norm: rep.function_norm,
            solution_norm: rep.solution_norm,
            update_norm: rep.update_norm,
            history: hist,
            tolerances: (rep.atol, rep.rtol, rep.stol, rep.divtol),
            limits: (rep.max_it, rep.max_f),
        })
    }
}

// ------------------------------------------------------------------- TAO

/// Result of a TAO solve.
#[derive(Clone, Debug, PartialEq)]
pub struct TaoOutcome {
    /// Iterations.
    pub iterations: usize,
    /// `TaoConvergedReason`.
    pub reason: i32,
    /// `(gatol, grtol, gttol)`.
    pub tolerances: (f64, f64, f64),
    /// Maximum iterations.
    pub max_iterations: i64,
    /// Final objective value (`TaoGetSolutionStatus`).
    pub objective: f64,
    /// Final gradient norm (`TaoGetSolutionStatus`: the norm TAO compares
    /// with `gatol`; the projected gradient of a bounded solve).
    pub gradient_norm: f64,
}

/// A PETSc optimisation solver (upstream `NonLinearSolverTAO`'s `Tao`).
#[derive(Debug)]
pub struct Tao {
    native: Native,
}

impl Tao {
    /// `TaoCreate`, name, `TAOGPCG`, `TaoSetFromOptions` (global database,
    /// as upstream's constructor). Collective.
    pub fn create(
        nc: &NativeComm<'_>,
        comm: &Communicator,
        session: &Session,
        name: &str,
    ) -> Result<Self> {
        let _g = PetscLock::enter_native(nc);
        check_capability(nc, comm, "create")?;
        let cname = c_string(name, "PETSc TAO name")?;
        let mut p = std::ptr::null_mut();
        // SAFETY: live communicator of the agreed native entry; valid name
        // and out-pointer.
        check(
            unsafe { ffi::akantu_petsc_tao_create(nc.raw(), cname.as_ptr(), &raw mut p) },
            "TaoCreate",
        )?;
        Ok(Self {
            native: Native::new(p, Kind::Tao, comm, session)?,
        })
    }

    /// Private database pushed around `TaoSetFromOptions`, then the global
    /// one. Collective.
    pub fn set_from_options(
        &mut self,
        nc: &NativeComm<'_>,
        options: Option<&Options>,
    ) -> Result<()> {
        let _g = PetscLock::enter_native(nc);
        check_capability(nc, &self.native.comm, "set_from_options")?;
        let o = options.map_or(std::ptr::null_mut(), Options::ptr);
        // SAFETY: live TAO; `o` null or a live options database.
        check(
            unsafe { ffi::akantu_petsc_tao_set_from_options(self.native.ptr.0, o) },
            "TaoSetFromOptions",
        )
    }

    /// The native part of upstream `NonLinearSolverTAO::solve`: `x` zeroed,
    /// objective/gradient and Hessian (`H` = the backend's matrix) routed
    /// to `callbacks`, optional bounds on the increment (owned rows),
    /// `TaoSolve`. Must run inside an agreed native region. `aliased`:
    /// trial points copied into `x` (TAO's solution vector), UD-428.
    pub fn solve(
        &mut self,
        nc: &NativeComm<'_>,
        hessian: &Matrix,
        x: &mut Vector,
        bounds: Option<(&[f64], &[f64])>,
        callbacks: &mut dyn NativeCallbacks,
        aliased: bool,
    ) -> Result<TaoOutcome> {
        let _g = PetscLock::enter_native(nc);
        check_capability(nc, &self.native.comm, "solve")?;
        for comm in [&hessian.native.comm, &x.native.comm] {
            check_capability(nc, comm, "Tao::solve operands")?;
        }
        let n = x.n_owned;
        check_layout(hessian, &[x], "petsc::Tao::solve")?;
        if let Some((lo, hi)) = bounds
            && (lo.len() != n || hi.len() != n)
        {
            return Err(Error::DimensionMismatch {
                context: "petsc::Tao::solve (bounds)",
                expected: n,
                got: lo.len().min(hi.len()),
            });
        }
        let (lo, hi) = bounds.map_or((std::ptr::null(), std::ptr::null()), |(l, h)| {
            (l.as_ptr(), h.as_ptr())
        });
        let mut rep = ffi::TaoReport::default();
        let mut user: UserData<'_> = callbacks;
        let user_ptr: *mut UserData<'_> = &raw mut user;
        // SAFETY: live TAO, matrix and vector of one communicator and
        // layout; `lo`/`hi` null or `n` readable doubles each, copied into
        // PETSc vectors before TaoSolve; `user_ptr` points to `user` on this
        // frame for the whole TaoSolve (trampolines run on this thread);
        // `rep` is a valid out-pointer.
        let rc = unsafe {
            ffi::akantu_petsc_tao_solve(
                self.native.ptr.0,
                hessian.ptr(),
                x.ptr(),
                lo,
                hi,
                callbacks::objective,
                callbacks::jacobian,
                user_ptr.cast::<c_void>(),
                len_i64(n, "owned rows")?,
                c_int::from(aliased),
                &raw mut rep,
            )
        };
        check(rc, "TaoSolve")?;
        Ok(TaoOutcome {
            iterations: usize::try_from(rep.iterations).unwrap_or(0),
            reason: rep.reason,
            tolerances: (rep.gatol, rep.grtol, rep.gttol),
            max_iterations: rep.max_it,
            objective: rep.fval,
            gradient_norm: rep.gnorm,
        })
    }
}
