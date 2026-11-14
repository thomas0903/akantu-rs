# `unsafe` sites of `akantu-python`

Every `unsafe` site of this crate is listed in the site table below, one
row per site (`scripts/unsafe_inventory.py --check`, run by `cargo test -p
akantu-validation --test unsafe_inventory`, fails when a site is missing,
moved or gone; `--update-lines` rewrites the Line column after an edit that
only moves code). The detail tables further down state each invariant in
full and are keyed by the same ids.
Each row is also pinned to a fingerprint of its site's code (comments and
whitespace ignored) in `UNSAFE.fingerprints`: a site
whose code changed fails the check, and `--update-lines` does not move its
row, until the row is reviewed and re-pinned with `--refresh <Id>`.
A fingerprint covers the Rust site's own tokens only: not the code around
it that sets up its invariants, and not C sources, which the guard
does not read; a change there shows only in the commit's diff and its
review.

**Not the only `unsafe` crate.** The FFI edges of `akantu-comm` (MPI),
`akantu-linalg` (MUMPS, PETSc), `akantu-mesh` (Scotch), `akantu-parser`
(libm) and `akantu-capi` (one export) have their own `UNSAFE.md`.
`akantu-core`, which owns the export storage and the phase protocol these
sites rely on, is `#![forbid(unsafe_code)]` (`crates/akantu-core/UNSAFE.md`
lists the guarantees). This crate is `#![deny(unsafe_code)]`; `src/lib.rs`
re-allows it, with a justification comment, for exactly three modules:

| `lib.rs` | Module | Why |
|---|---|---|
| `#[allow(unsafe_code)]` on `foreign_view` | `src/foreign_view.rs` | lease dereferences after the phase check |
| `#[allow(unsafe_code)]` on `scoped` | `src/scoped.rs` | phase-scoped lifetime erasure; the running Python frame (`scoped::python`, feature `python`) |
| `#[allow(unsafe_code)]` on `py` (feature `python`) | `src/py/**` | PyO3's `#[pymodule]`/`#[pyclass]`/`#[pymethods]` expand to `unsafe` FFI glue; **no hand-written `unsafe`** under `py/` (enforced by `tests/binding_rules.rs`; the inventory guard scans the source text, so macro expansions are not sites) |

**Counts**: block 11, `unsafe impl` 6 — **17 library sites, no test site**
(the `compile_fail` doc tests of `src/scoped.rs` contain no `unsafe`).

## Site table

The invariant column summarises; the full text is the detail row with the
same id.

### src/foreign_view.rs

| Id | Line | Site | What it does | Invariant (summary; detail below) | Established by | Test |
|---|---|---|---|---|---|---|
| PY-01 | 94 | `impl<T: Copy + Send> Send for ForeignView<T>` impl 1 | a view (lease + integers) may move between threads | every dereference takes `&Interpreter` and passes `check_foreign_access` first; the token is thread-confined | `akantu-core` phase protocol (`Interpreter: !Send + !Sync`, exclusive per runtime) | `tests/foreign_views.rs` (Miri) |
| PY-02 | 97 | `impl<T: Copy + Send> Sync for ForeignView<T>` impl 1 | shared views across threads | `&ForeignView` exposes metadata and interpreter-gated accesses only | as PY-01 | as PY-01 |
| PY-03 | 205 | `impl<T: Copy> ForeignView<T> › fn read` block 1 | `lease.as_ptr().add(k).read()` | `k` inside the leased window; admission checked | `offset` (runtime bounds check), `check_foreign_access` (runtime), the lease (type) | `tests/foreign_views.rs` (Miri, SB + TB) |
| PY-04 | 214 | `impl<T: Copy> ForeignView<T> › fn write` block 1 | `…add(k).write(value)` | as PY-03; `check_writable` first | as PY-03 | as PY-03 |
| PY-05 | 227 | `impl<T: Copy> ForeignView<T> › fn fill` block 1 | writes every value of the window | indices inside the window; admission checked once (interpreter stays borrowed) | the window invariant (`rows` checks sub-ranges), `check_foreign_access` | as PY-03 |
| PY-06 | 250 | `impl<T: Copy> ForeignView<T> › fn copy_to` block 1 | `ptr::copy_nonoverlapping` out of the lease | source inside the lease; `out` cannot overlap (no Rust slice of an exported block exists without a native guard) | length check of `out`, the admission, `akantu-core`'s guard protocol | as PY-03 |
| PY-07 | 580 | `mod numpy_export › fn to_numpy` block 1 | `PyArray_NewFromDescr` + `PyArray_SetBaseObject` (NumPy handover) | dims/strides/pointer describe exactly the window per layout; the lease object becomes the array's base; read-only views not `WRITEABLE`; reference counts per NumPy's stealing rules | the layout checks (`Vector` needs 1 component, `Tuple` 1 row), NumPy's base-object keep-alive; residual: NumPy releasing the GIL (UD-085) | pytest suite (`python/tests`), `cargo test -p akantu-python --features python` |

### src/scoped.rs

| Id | Line | Site | What it does | Invariant (summary; detail below) | Established by | Test |
|---|---|---|---|---|---|---|
| PY-08 | 234 | `impl<T: ?Sized + Sync + 'static> Send for Scoped<T>` impl 1 | a shared lend handle may move | `&T` only inside `with`, while the use is counted and the lending call is alive, for the lend's own `ContextKey`; `T: Sync` | `Scoped` scope state (mutex), `ContextKey` (runtime check), `LendContext::admit` (frame chain) | `tests/foreign_views.rs`, `tests/v119_scoped_adversarial.rs` (Miri) |
| PY-09 | 236 | `impl<T: ?Sized + Sync + 'static> Sync for Scoped<T>` impl 1 | shared references to the handle | as PY-08 (`with(&self, …)` hands out `&T` only) | as PY-08 | as PY-08 |
| PY-10 | 262 | `impl<T: ?Sized + 'static> Scoped<T> › fn with` block 1 | `self.ptr.as_ref()` | use admitted while the scope is open, from the lending context; in flight until `f` returns; scope end waits | the scope mutex, `begin_use` (key check), `EndScope` | as PY-08 |
| PY-11 | 323 | `fn with_model_services` block 1 | `transmute` of `*const (dyn ModelServices + 'a)` to `'static` | only the lifetime bound changes; `'static` never relied upon (every dereference through `Scoped::with`) | `Scoped::with` admission (PY-10) | `src/py/runtime.rs` unit tests, pytest callbacks |
| PY-12 | 449 | `impl<T: ?Sized + 'static> Send for ScopedMut<T>` impl 1 | an exclusive lend handle may move | dereferences only in `with_mut` on the lending thread, for the lend's key, while the lending call runs, with no other use in flight | `ExclusiveScope::begin_use` (thread, key, open, in-use checks) | `tests/foreign_views.rs` (`scoped_mut_*`), `v119_scoped_adversarial` |
| PY-13 | 452 | `impl<T: ?Sized + 'static> Sync for ScopedMut<T>` impl 1 | shared references to the handle | as PY-12 | as PY-12 | as PY-12 |
| PY-14 | 481 | `impl<T: ?Sized + 'static> ScopedMut<T> › fn with_mut` block 1 | `&mut *self.ptr.as_ptr()` | the only reference for the duration of `f`; the lender does not use its borrow meanwhile; `EndExclusive` waits | as PY-12 | as PY-12 |
| PY-15 | 571 | `fn with_solver_access` block 1 | `transmute` of `*mut (dyn SolverAccess + 'a)` to `'static` | only the lifetime bound changes; every dereference through `with_mut` | `ScopedMut::with_mut` admission (PY-14) | `solver_access_*` tests (Miri) |
| PY-16 | 606 | `mod python › fn current_frame` block 1 | `PyEval_GetFrame()` (feature `python`) | `py: Python<'_>` proves the thread is attached; returns a borrowed frame or NULL; stable ABI | PyO3's `Python` token (type) | `src/py/runtime.rs` unit tests (real interpreter), `python/tests/test_execution_contexts.py` |
| PY-17 | 611 | `mod python › fn current_frame` block 2 | `Bound::from_borrowed_ptr_or_opt(py, frame)` | NULL or the valid borrowed frame just returned, alive while this code runs inside it; a strong reference taken at once | the C API's contract (borrowed reference of the running frame) | as PY-16 |

## The protocol every site relies on

From `akantu_core::array_shared` (safe code, `docs/ARCHITECTURE.md`, "Shared storage and its aliasing rules"):
if `ExportLease::check_foreign_access(interp)` returns `Ok(())`, then while
`interp` stays borrowed no native guard (`ArrayRef`, `ArrayMut`, `SliceRef`)
of any allocation exported to that runtime is alive or can be acquired, and
no other thread performs foreign accesses on it. From
`akantu_core::array_storage`: a lease keeps its allocation (at least
`nb_values()` initialised values) alive until it drops; a block is never
reallocated under a lease (growth beyond capacity, any shrink, replacement
or destruction retire it into the lease's anchor).

## Detail: `src/foreign_view.rs`

| Id | Site | Invariant |
|---|---|---|
| PY-01 | `unsafe impl<T: Copy + Send> Send for ForeignView<T>` | a view is a lease (`Arc` keep-alive) plus integers; moving it performs no value access. Every dereference takes `&Interpreter` and first passes `check_foreign_access`; the interpreter token is `!Send + !Sync` and exclusive per runtime, so dereferences are confined to the thread holding it and ordered with native guards by the phase word's acquire/release CAS. `T: Send` because values are copied out on that thread. |
| PY-02 | `unsafe impl<T: Copy + Send> Sync for ForeignView<T>` | as above: `&ForeignView` exposes metadata, and accesses that need the thread-confined interpreter. |
| PY-03 | `ForeignView::read`: `lease.as_ptr().add(k).read()` | `k` from the checked `offset(i, j)`: `k < (first_row + rows) · nbc ≤ nb_values` (the row window lies inside the lease, checked by `ForeignView::rows`); admission checked; `T: Copy` initialised values. |
| PY-04 | `ForeignView::write`: `…add(k).write(value)` | as `read`; `check_writable` first; the write targets the leased block only. |
| PY-05 | `ForeignView::fill`: `…add(k).write(value)` per value | `k ∈ [first_row · nbc, (first_row + rows) · nbc)`; admission checked once before the loop (the interpreter stays borrowed). |
| PY-06 | `ForeignView::copy_to`: `ptr::copy_nonoverlapping` | source range inside the lease; `out` cannot overlap it: a Rust slice of an exported allocation exists only under a native guard, and the admission proves none is alive (retired blocks are reachable only through leases). |
| PY-07 | NumPy handover (`PyArray_NewFromDescr` + `PyArray_SetBaseObject`) | `data`, `dims`, `strides` describe exactly the view's window for each layout — `Rows`/`ColumnMajor`: `data + i·nbc + j`, `i < rows`, `j < nbc`; `Vector` (`nbc == 1`): `data + i`; **`Tuple` (`rows == 1`, enforced): dims `[nbc]`, strides `[size_of::<T>()]`, pointer `lease + first_row · nbc`** (used for the live Dirichlet rows) — all inside the lease window. The `_ArrayLease` object owning the view becomes the array's base, so NumPy keeps the lease alive for the array and every array derived from it; on base-installation failure the array is released before returning. Read-only views get no `WRITEABLE` flag. Reference counting: `PyArray_NewFromDescr` steals the descriptor reference (`into_dtype_ptr` increments it), `PyArray_SetBaseObject` steals the base reference also on failure. Foreign accesses happen only in the foreign phase because the binding never releases the GIL inside a native section; the residual multi-threaded obligation (NumPy releasing the GIL on an akantu view) is UD-085's. |

## Detail: `src/scoped.rs`

**Execution contexts.** Every lend
belongs to a `ContextKey` (fresh per lend, never reused) and every use
(`Scoped::with`, `ScopedMut::with_mut`) presents the key of the execution
context its caller runs in; `begin_use` refuses a key that is not the
lend's own. The bindings present a lend's key only after
`LendContext::admit` (`src/py/runtime.rs`, safe code) found the **lending
frame** — the Python frame that made the binding call whose native code
lends, read with the C API `PyEval_GetFrame` when the callback starts —
in the frame chain of the running thread state (`PyEval_GetFrame`, then
the read-only `f_back` attribute of the immutable built-in frame type).
The rows below rely on this property ("**admitted only from the lending
context**"): such code runs nested inside the lending call, **on the C
stack that holds it**, so the lent pointer — into that stack or anywhere
else — designates the lent value. See "Execution contexts: the lending-frame check" for the
argument and what is refused.

| Id | Site | Invariant |
|---|---|---|
| PY-08 | `unsafe impl<T: ?Sized + Sync + 'static> Send for Scoped<T>` | a `Scoped<T>` only yields `&T` inside `with`, while the use is counted in flight, and a use is admitted only while the lending call (hence the borrow) is alive and only for a caller presenting the lend's `ContextKey` (the bindings: only from the lending context); `T: Sync` makes shared access from another thread sound; the scope end waits for in-flight uses. `T: Send` not needed (the handle never moves, mutates or drops the `T`). |
| PY-09 | `unsafe impl … Sync for Scoped<T>` | as above; `with(&self, key, f)` only hands out `&T`. |
| PY-10 | `Scoped::with`: `self.ptr.as_ref()` | the use was admitted while the scope was open and **from the lending context** (key check; the bindings present the key only from code running inside the lending call, on its own stack, so a stack local of the lending call designates the lent value — a greenlet that copied that stack away runs in another context and is refused); `_use` keeps it in flight until `f` returns or unwinds; the scope end does not return while a use is in flight; admission, release and close are ordered by the scope mutex. |
| PY-11 | `with_model_services`: `mem::transmute` of `*const (dyn ModelServices + 'a)` to `'static` | only the trait-object lifetime bound changes (same fat-pointer layout); the `'static` bound is never relied upon: every dereference goes through `Scoped::with`, which admits a use only while this call runs and only from its execution context. |
| PY-12 | `unsafe impl<T: ?Sized + 'static> Send for ScopedMut<T>` | a `ScopedMut<T>` (exclusive lend of a `&mut T`, `with_scoped_mut` / `with_solver_access`) dereferences its pointer only inside `with_mut`, after `ExclusiveScope::begin_use` checked that the current thread is the **lending thread**, that the caller presents the lend's `ContextKey`, that the lending call is still running and that no other use is in flight. Another thread can only clone/drop the handle (an `Arc` of a `Mutex`/`Condvar` state) or have its uses refused, so neither `T: Send` nor `T: Sync` is needed: every access to the `T` happens on the thread that lent it. |
| PY-13 | `unsafe impl … Sync for ScopedMut<T>` | as above: `&ScopedMut<T>` allows `with_mut` (thread- and context-checked, exclusive) and `is_valid` (reads the scope flag under its mutex). |
| PY-14 | `ScopedMut::with_mut`: `&mut *self.ptr.as_ptr()` | the use was admitted on the lending thread, **from the lending context** (the bindings: only from code running inside the lending call on its own stack, see above), while the lending call — and the `&'a mut T` the handle was made from — was alive; the lender hands that borrow over for the whole call and does not use it while `f` runs; every other use is refused while this one is in flight (`in_use`; nested uses from inside `f`, uses from other threads and from other contexts fail without waiting), so the `&mut T` is the only reference for the duration of `f`; `EndExclusive` closes the scope and does not return while a use is in flight; admission, release and close are ordered by the scope mutex. Unwinding out of `f` ends the use (`ExclusiveUse::drop`). |
| PY-15 | `with_solver_access`: `mem::transmute` of `*mut (dyn SolverAccess + 'a)` to `'static` | only the trait-object lifetime bound changes (same fat-pointer layout); the `'static` bound is never relied upon: every dereference goes through `ScopedMut::with_mut` (above), which admits a use only on this thread, from this call's execution context, while this call (so `'a`) runs, and `_end` keeps `'a` alive until the use in flight ends. The closure gets `&mut (dyn SolverAccess + 'static)` for the use only; the trait's methods return nothing that outlives that borrow (`dofs()` returns `&mut DofManager` tied to it) and an unsized trait object cannot be moved out. |
| PY-16 | `python::current_frame`: `pyo3::ffi::PyEval_GetFrame()` (feature `python`) | `py: Python<'_>` proves the thread is attached; the function takes no argument and returns a borrowed reference to the thread state's current frame object or NULL (no Python frame; on 3.11+ also when the frame object cannot be created, CPython clearing that error itself). Part of the stable ABI (the wheel is `abi3-py39`). |
| PY-17 | `python::current_frame`: `Bound::from_borrowed_ptr_or_opt(py, frame.cast())` | the pointer is NULL (→ `None`) or the valid borrowed `PyFrameObject *` just returned, owned by the running frame, which is alive while this code runs inside it; a new strong reference is taken at once. The frame chain is then read with `getattr(frame, "f_back")` on that genuine frame object (safe code: a getset of the immutable built-in frame type, no user code runs). |

## Uses added by the phase-field and derived-law bindings

**No new `unsafe`, no change of `src/scoped.rs` or `src/foreign_view.rs`**
(so the Miri runs of the lending layer below still apply; `tests/binding_rules.rs`
still finds no hand-written `unsafe` under `src/py/`). How the existing
sites are used:

* **A second state type lent `&mut`:** the callbacks of a Python
  `PhaseField` subclass (`src/py/pf_laws.rs`, `PyPfMethods::lend`) lend the
  runtime phase field's state with `with_scoped_mut` as
  `ScopedMut<akantu::phase_field::ForeignPhaseFieldState>` (a sized,
  `'static` type), inside the callback's `LendContext` (`lending_callback`),
  exactly as `PyLawMethods::lend` lends a `Material` state; every use goes
  through `ScopedMut::with_mut` with the admitted key (`PfLink::lent_top` →
  `LendContext::admit`), and the parameter registry carries the admitted
  lend in `Prepared` (second field) from `prepare` (frame check, before the
  native section) to `with_registry` (inside it), as for `Material`.
  Rows 449 / 452 / 481 cover it unchanged (the `T: ?Sized + 'static`
  generic; the state is not touched by the lender during the callback:
  `ForeignPhaseField::with_methods` takes the behaviour object out, the
  state is borrowed `&mut` for the whole call).
* **A third `dyn RuntimeLawState` implementation:** a Python
  `MaterialLinearElastic<d>D` subclass is an `akantu::solid::DerivedMaterial`
  whose state `DerivedLawState` (the built-in elastic law, boxed as
  `dyn Material + Sync`, plus runtime parameters and internals) is lent as
  `ScopedMut<dyn RuntimeLawState>` / `Scoped<dyn RuntimeLawState>` by the
  free functions `lend_state` / `lend_state_shared` (the bodies of
  `PyLawMethods::lend` / `lend_shared`, generalised to any
  `&mut (dyn RuntimeLawState + 'static)`). `RuntimeLawState: Send + Sync`
  holds for it (`Box<dyn Material + Sync>`), so row 234's bound is met.
* **Exports through a lent solve access:** `SolverVector.getValues()`
  inside a solver callback exports the residual through the solve access
  the solve lent (`ScopedMut<dyn SolverAccess>::with_mut` → `dofs()` →
  `DofManager::residual_shared_mut()` → `ExportTable::view`): an ordinary
  export under `&mut` of the storage (no native guard can be alive: the
  suspended solve lent its DOF manager exclusively), as the existing
  `with_dofs` mutations through the same access.
* **New exported arrays** (ordinary `SharedArray` storage, the existing
  export path): the DOF manager's residual (`ModelKey::Residual`,
  `akantu_solver::ResidualVector`) and the phase-field model's internal
  force (`ModelKey::Nodal("internal_force")`).

## Uses added by the cohesive-law and structural bindings

**No new `unsafe`, no change of `src/scoped.rs` or `src/foreign_view.rs`.**
How the existing sites are used:

* The law state lent to a Python law callback is now a trait object:
  `Lent::Mut { state: ScopedMut<dyn akantu_solid::RuntimeLawState>, … }`
  and `Lent::Shared(Scoped<dyn RuntimeLawState>)` (was
  `ScopedMut<ForeignLawState>` / `Scoped<ForeignLawState>`). The sites of
  `src/scoped.rs` are generic over `T: ?Sized + 'static` and already lend
  `dyn ModelServices`; a trait object only makes the erased pointer a fat
  `NonNull` (`NonNull::from(value)`), with the same scope, admission and
  execution-context checks. `RuntimeLawState: Send + Sync` (so `Scoped<dyn
  RuntimeLawState>: Send` by the existing `T: Sync` bound). The lent value
  is `&mut ForeignLawState` (a Python `Material`) or `&mut MaterialCohesive`
  (a Python `MaterialCohesive`, the runtime cohesive law; its behaviour
  object is taken out of the law for the call, so the lent law does not
  alias the trampoline running it).
* Every native → Python call of a runtime cohesive law goes through
  `lending_callback` (`PyCohesiveLawMethods::lend`, a copy of
  `PyLawMethods::lend`): the law is lent with `with_scoped_mut`, the model
  services with `with_model_services`, inside the callback's
  `LendContext`. The traction and tangent callbacks run after the caller
  dropped its guards on the exported nodal arrays (`akantu-cohesive`
  `compute_kinematics` / `compute_foreign_tractions`): the runtime refuses
  a foreign callback while native borrows are alive (it would raise that
  refusal instead of calling Python).
* `checkInsertion` of a Python cohesive law is **not** a callback: the
  binding runs it between two native calls, with the model released
  (`check_cohesive_stress_prepare` / `_finish`); nothing is lent. The
  inserter's insertion flags are moved into a `SharedElementTypeMapArray`
  for that window and exported through the ordinary `ModelKey` path
  (`ModelKey::InsertionFacets`), then moved back (`SharedArray::into_inner`
  retires an exported block: views stale but safe).
* New exported arrays (ordinary `SharedArray` storage, the existing export
  path): the structural model's `external_force`, `internal_force` and
  `element_material` blocks (`ModelKey::Nodal`, `ModelKey::ElementMaterial`,
  `u32` views), the contact model's seven fields upstream returns by
  reference (`ModelKey::Nodal`).

## Execution contexts: the lending-frame check

**The problem.** The exclusive lends (below) lend pointers to stack locals of the lending call
(`DelegateAccess` / `TypeAccess` / `CallbackAccess` in `py/solver.rs`, the
solid model's `services` local lent by `py/laws.rs`) and stores the
handles where any Python code of the thread finds them again
(`ModelHandle.access` / `.services`, `LawLink.lent`). A greenlet switch
inside a callback keeps the lending call alive but copies its stack slice
away; the other greenlet's frames then reuse the addresses and a use
dereferenced foreign stack data (adversarial programs A1: `SIGSEGV`; A2:
garbage). The thread / open-scope / in-use checks cannot see a C-stack
switch.

**Mechanism chosen: the lending frame must be in the running frame
chain.** At the start of each callback (`runtime::lending_callback`) the
bindings record a `LendContext`: a fresh `ContextKey` and the Python
frame that is running at that moment — the frame that called the binding
whose native code is calling back; it stays suspended inside that binding
call for the whole lend. Every lend made for the callback uses that key;
every lookup of a lent handle (`ModelHandle::active_access`,
`active_services`, `LawLink::lent_top`, `RegistryAccess::prepare`) calls
`LendContext::admit`, which presents the key only if the lending frame is
in the frame chain of the running thread state; otherwise the binding call
raises `akantu.Exception` ("… made from another execution context than
that callback …"), like a use from another thread (UD-085). `begin_use`
additionally refuses a key that is not the lend's own.

**Soundness argument.** (1) A frame that is executing belongs to the frame
chain of exactly one execution context of a thread state: the one that
called it. CPython links each new frame to the thread state's current
frame; C-stack switching libraries must save and restore that chain with
the C stack (otherwise CPython's own frame stack would be corrupted), and
start a new context with an empty chain — a chain spliced onto another
context's frames would dangle as soon as that context returned from
them, so no working switcher does it (pinned for greenlet by
`python/tests/test_execution_contexts.py::test_greenlet_contexts`: a
greenlet started inside the lending call is refused). A generator,
coroutine or async task frame is linked to whoever resumes it, for that
resumption only. (2) The lending frame executes the binding call for the
whole lend, so it is in the chain only of the context that made the
call, and only below the frames nested in that call. (3) Code that finds
it in its chain therefore runs nested inside the lending call, on the C
stack that carries it: the lending call's stack is live and in place, so
the lent pointer designates the lent value (the protocol checks above —
thread, open scope, exclusive use — then apply as for every lend). (4) The
current frame comes from the C API, not from `sys._getframe` (which user
code, debuggers or profilers can replace), and `f_back` of a genuine frame
object runs no user code, so Python code cannot make the check see
another chain (`test_a_patched_sys_getframe_does_not_change_the_answer`).
**Threat model:** the admission is sound
against Python code that stays within the language's object model
(threads, generators, coroutines, copied `contextvars` contexts, trace
and profile hooks, finalizers, signal handlers, a replaced
`sys._getframe`, and stack switchers such as greenlet that keep CPython's
frame chain consistent). It is not a defence against code that rewrites
interpreter internals (`ctypes`, raw-address buffers, edits through
`gc.get_referents`, C extensions): such code can corrupt memory without
the bindings' help, and claim (4) does not extend to it.
Alternatives rejected: greenlet identity (depends on greenlet being
importable, blind to other switchers); a `contextvars` token (copied
contexts carry it — `test_copied_contextvars_contexts_carry_no_admission` —
and greenlet lets two greenlets share one `Context` through `gr_context`);
lending from a heap allocation (the lent objects borrow other stack data
of the native callers, e.g. the solid model's services, so no heap copy
removes the stack target; not provable).

**What is refused** (a lend in flight, the caller not inside it): another
thread; another greenlet (also one started inside the callback, and the
main greenlet while the callback's greenlet is suspended); a generator,
coroutine or async task resumed outside the callback while it runs; code
run in a copied `contextvars` context outside the callback; a binding
called with no Python frame at all (a lend made without a running frame
admits nothing). **Admitted:** the callback and everything it calls,
including generators / coroutines resumed inside it, an event loop run
inside it, finalizers, trace hooks and signal handlers that run nested in
it. After the lending call every use is refused as before.

**Sites.** Two new `unsafe` blocks, both in `src/scoped.rs`
(`python::current_frame`, PY-16 and PY-17); none under `src/py/`
(`tests/binding_rules.rs`), none in `src/foreign_view.rs`. Rows PY-08…PY-15
gained the context condition (the code of those blocks is unchanged
except the key parameter of `with` / `with_mut` and the lends).

**Tests.** `tests/foreign_views.rs` (Miri): `context_keys_are_unique`,
`scoped_mut_refuses_another_execution_context`,
`scoped_refuses_another_execution_context`,
`solver_access_lent_from_the_stack_refuses_another_context`;
`tests/v119_scoped_adversarial.rs` (Miri): six adversarial
tests, adapted to keys; the compile-fail probes CF1–CF3 as
`compile_fail` doctests of `src/scoped.rs`, each after a compiling
control; `src/py/runtime.rs` unit tests (feature `python`, real
interpreter): a lend without a Python frame admits nothing; admitted in
the call and nested code, refused in another thread, after the call and in
a generator resumed outside; `python/tests/test_execution_contexts.py`:
the probe cases (generators, coroutines / asyncio, threads, copied
contexts, patched `sys._getframe`, greenlets) and the bindings: the adversarial
programs A1 (four actions) and A2 with greenlet (skipped without it), and
generator, thread and coroutine variants that need no extra package.

Miri, run when the check was added (`ci/tiers/miri.sh` recipe,
`nightly-2026-10-01`, rustc 1.101.0-nightly 21b707e3f): `cargo miri test
-p akantu-python --locked` under Stacked Borrows and under Tree Borrows
(`MIRIFLAGS=-Zmiri-tree-borrows`): lib 1 passed, `binding_rules` 3 ignored
(static source checks), `tests/foreign_views.rs` **25 passed** (including the 4
context tests), `tests/v119_scoped_adversarial.rs` **6 passed**, doctests 8
passed (3 controls + 3 CF probes + 2 of `foreign_view.rs`); no undefined
behaviour reported. Miri cannot run the interpreter: the frame-chain check
itself is shown by the Python-linked unit tests and the pytest cases
(greenlet included) listed above.

## Exclusive lends (`ScopedMut`)

Four sites in `src/scoped.rs` (PY-12…PY-15 above): the **exclusive**
lend `ScopedMut<T>` (`with_scoped_mut`, `with_solver_access`, trait
`SolverAccess`). It gives a Python solver callback (`InterceptSolverCallback`,
a `SolverCallback` subclass replacing the model's) access to the running
solve — `self.model.assembleMatrix("K")`, `model.getDOFManager()…` — while
the solver holds the model mutably: the solve lends its `&mut` callback /
DOF manager for the duration of one callback and the handle refuses every
use outside it (after the call, on another thread, re-entrant). No `unsafe`
was added under `src/py/` (`tests/binding_rules.rs`), none in
`src/foreign_view.rs`. Tests in `tests/foreign_views.rs`:
`scoped_mut_is_exclusive_and_dies_with_the_lending_call`,
`scoped_mut_refuses_other_threads`, `solver_access_handle_reaches_the_running_solve_only_during_the_call`.

Miri, run when these sites were added (the `ci/tiers/miri.sh` recipe
with `nightly-2026-10-01`, rustc 1.101.0-nightly):
`cargo miri test -p akantu-python --locked` under Stacked Borrows and under
Tree Borrows (`MIRIFLAGS=-Zmiri-tree-borrows`): lib 1 passed, the 3
static-source checks of `tests/binding_rules.rs` ignored under Miri,
`tests/foreign_views.rs` **21 passed** (including the 3
`ScopedMut` tests), doctests 2 passed; no undefined behaviour reported.

## Slice guards (`SliceRef`)

No `unsafe` block. The native guard `SliceRef`
(`akantu-core`, safe code) narrows an `ArrayRef` to a slice and keeps its
guard count, so it is covered by the admission protocol above like
`ArrayRef`; the export anchor's live tuple count is an atomic read
by safe code only.

Miri: `cargo +nightly miri test -p akantu-python` (lib + `tests/foreign_views.rs`
+ doctests, Stacked and Tree Borrows); the PyO3/NumPy code and the
model/mesh crates do not run under Miri (`docs/TESTING.md`).

## The MPI build and Python overrides

No `unsafe` block: `src/scoped.rs` / `src/foreign_view.rs` are the same
in the MPI build. The dispatch to a Python override calls
the Python override with PyO3's safe API under the lend the callback
already holds (D-086 admission checked first); the MPI build (feature
`mpi`) uses the safe `akantu::runtime` / `akantu::comm` APIs (the MPI and
Scotch FFI edges stay in `akantu-comm` and `akantu-mesh`, unchanged).
