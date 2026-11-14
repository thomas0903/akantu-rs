// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): contract of python/py_solver.cc:85-92
// (`SolverVector.getValues()`: a view of the residual's `Array<Real>`, "Is
// not copied") on src/model/common/dof_manager/dof_manager_default.cc (the
// residual assemblies write the array in place). Exportable storage:
// akantu-rs (D-047, PY-045).

//! The residual's values are exportable storage: an export lease (what the
//! Python view holds) stays attached through every assembly, zeroing and
//! matrix product, and is retired (stale but safe) when a renumbering moves
//! the values.

mod common;

use akantu_comm::Communicator;
use akantu_core::array::ArrayView;
use akantu_core::array_shared::ForeignRuntime;
use akantu_solver::DofManager;

use common::my_model::gen_mesh;

#[test]
fn exported_residual_follows_assemblies_and_retires_on_a_renumbering() {
    let mesh = gen_mesh(4); // 4 nodes
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_nodal_dofs(&mesh, "u", 2).unwrap();
    assert_eq!(dm.residual().local_size(), 8);
    let rt = ForeignRuntime::new();
    let mut interp = rt.enter().unwrap();
    let lease = {
        let _s = interp.native().unwrap();
        dm.residual_shared_mut().export(&rt).unwrap()
    };
    assert_eq!((lease.len(), lease.nb_component()), (8, 1));
    let ones = [1.0; 8];
    {
        let _s = interp.native().unwrap();
        dm.assemble_to_residual("u", ArrayView::new(&ones, 2).unwrap(), 0.5)
            .unwrap();
        assert_eq!(dm.residual().values(), [0.5; 8]);
        assert!(dm.residual_shared_mut().is_attached(&lease));
        // lumped product r += M x, then the lumped solve reads r
        dm.new_lumped_matrix("M").unwrap();
        dm.assemble_to_lumped_matrix("u", ArrayView::new(&[2.0; 8], 2).unwrap(), "M", 1.0)
            .unwrap();
        dm.assemble_lumped_mat_mul_vect_to_residual("u", "M", &ones, 1.0)
            .unwrap();
        assert_eq!(dm.residual().values(), [2.5; 8]);
        assert!(dm.residual_shared_mut().is_attached(&lease));
        dm.zero_residual();
        assert_eq!(dm.residual().values(), [0.0; 8]);
        assert!(dm.residual_shared_mut().is_attached(&lease));
    }
    assert!(!lease.is_retired());
    {
        // a second family renumbers the equations: 8 → 20, the values move
        let _s = interp.native().unwrap();
        dm.register_nodal_dofs(&mesh, "v", 3).unwrap();
        assert_eq!(dm.residual().local_size(), 20);
        assert!(!dm.residual_shared_mut().is_attached(&lease));
    }
    assert!(lease.is_retired(), "the old block is retired, not freed");
    assert_eq!(lease.len(), 8);
}

#[test]
fn exported_residual_refuses_native_access_outside_a_native_section() {
    let mesh = gen_mesh(3);
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_nodal_dofs(&mesh, "u", 1).unwrap();
    let rt = ForeignRuntime::new();
    let mut interp = rt.enter().unwrap();
    let _lease = {
        let _s = interp.native().unwrap();
        dm.residual_shared_mut().export(&rt).unwrap()
    };
    // foreign phase: the residual's guard is refused (the guarded accessor
    // panics like a conflicting `RefCell` borrow)
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = dm.residual().values().len();
    }));
    assert!(r.is_err());
    drop(interp);
}
