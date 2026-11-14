// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/dof_manager/
// dof_manager.cc:276-367 (registerDOFs on a support group), :646-672
// (onNodesAdded filtered by the support group), dof_manager_inline_impl.hh:
// 230-302 (assembleElementalMatricesToMatrix_: filter, else the support
// group's elements), :305-330 (assemblePreassembledMatrix with two DOF
// families), src/model/common/non_linear_solver/non_linear_solver_lumped.cc:
// 42-95 (b_a2x), src/model/common/model_solver.cc:76-217 (initDOFManager
// from a parser section: error branches).

//! `g02_*` (group-supported nodal DOFs), `g03_*`
//! (pre-assembled terms across two DOF families), `g07_*` (`b_a2x` of the
//! lumped solver), `g08_*` (error branches of `init_from_section`) and `g14_*`
//! (solver internal data released before the communicator finalises,
//! `src/solver/sparse_solver.cc:38,70`). Every expectation is derived by
//! hand from the upstream code cited above.

mod common;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use akantu_comm::Communicator;
use akantu_core::enums::{MatrixType, NonLinearSolverType, SparseSolverType};
use akantu_core::{Array, Element, Error, GhostType, GlobalIdx, Idx, Real, Result};
use akantu_linalg::direct::{BackendCapabilities, SparseDirectSolver, centralized_capabilities};
use akantu_linalg::{SparseMatrixAij, TermsToAssemble};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_parser::param::{ParamValue, ParameterRegistry, Parsable};
use akantu_parser::{Parser, ParserType};
use akantu_solver::model_solver::check_dof_manager_type;
use akantu_solver::{
    DofManager, ModelSolver, NonLinearSolver, NonLinearSolverLumped, SolverCallback,
};
use common::my_model::{MyModel, gen_mesh};
use common::{SEG, serial_bar};

/// Dense view of the stored entries `(row, col) → value` of a matrix.
fn entries(dm: &DofManager, id: &str) -> BTreeMap<(GlobalIdx, GlobalIdx), f64> {
    let (r, c, v) = dm.matrix(id).unwrap().coo();
    let mut out = BTreeMap::new();
    for ((&i, &j), &x) in r.iter().zip(c).zip(v) {
        *out.entry((i, j)).or_insert(0.0) += x;
    }
    out
}

/// Element group `g` of the 5-element bar holding elements 4 then 1 (not
/// sorted, not contiguous): its node group is `[4, 5, 1, 2]` in insertion
/// order (upstream `ElementGroup::add(element, add_nodes = true,
/// check_for_duplicate = true)`).
fn bar_with_group() -> Mesh {
    let mut mesh = serial_bar(5);
    let mut g = mesh.groups_mut().create_element_group("g", 1).unwrap();
    g.add(
        Element::new(SEG, 4, GhostType::NotGhost),
        Some(&[4, 5]),
        true,
    )
    .unwrap();
    g.add(
        Element::new(SEG, 1, GhostType::NotGhost),
        Some(&[1, 2]),
        true,
    )
    .unwrap();
    assert_eq!(g.nodes(), &[4, 5, 1, 2]);
    mesh
}

/// Column-major 4×4 element matrix with distinct entries `100k + 10i + j + 1`.
fn element_matrix(k: usize) -> [f64; 16] {
    let mut m = [0.0; 16];
    for j in 0..4 {
        for i in 0..4 {
            m[i + 4 * j] = (100 * k + 10 * i + j + 1) as f64;
        }
    }
    m
}

// ---------------------------------------------------------------

/// `registerDOFs(id, array, group)`: one tuple per node of the group's node
/// group, in node-group order; equations numbered in that order; the
/// associated node of every DOF is the group node. A whole-mesh family
/// registered afterwards is numbered after it.
#[test]
fn g02_group_supported_registration_counts_and_numbers() {
    let mesh = bar_with_group();
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_nodal_dofs_on_group(&mesh, "u", 2, "g").unwrap();
    let f = dm.family("u").unwrap();
    assert_eq!(f.group_support.as_deref(), Some("g"));
    assert_eq!((f.dofs.len(), f.dofs.nb_component()), (4, 2));
    assert_eq!(
        dm.local_equation_numbers("u").unwrap(),
        &[0, 1, 2, 3, 4, 5, 6, 7]
    );
    let global: Vec<Option<GlobalIdx>> = (0..8).map(Some).collect();
    assert_eq!(dm.global_equation_numbers("u").unwrap(), global);
    assert_eq!(f.associated_nodes, vec![4, 4, 5, 5, 1, 1, 2, 2]);
    assert_eq!((dm.system_size(), dm.local_system_size()), (8, 8));
    assert_eq!(dm.pure_local_system_size(), 8);

    dm.register_nodal_dofs(&mesh, "w", 1).unwrap();
    assert_eq!(
        dm.local_equation_numbers("w").unwrap(),
        &[8, 9, 10, 11, 12, 13]
    );
    assert_eq!(
        dm.family("w").unwrap().associated_nodes,
        vec![0, 1, 2, 3, 4, 5]
    );
    assert_eq!(dm.system_size(), 14);
    assert_eq!(dm.residual().values().len(), 14);

    // a per-DOF array of the group family goes to its equations
    let mut target = vec![0.0; 14];
    let values: Vec<f64> = (1..=8).map(f64::from).collect();
    dm.assemble_to_global_array("u", &values, &mut target, 2.0)
        .unwrap();
    let mut want = vec![0.0; 14];
    for (k, w) in want.iter_mut().take(8).enumerate() {
        *w = 2.0 * (k + 1) as f64;
    }
    assert_eq!(target, want);
}

/// `assembleElementalMatricesToMatrix` without a filter assembles the
/// support group's elements, in group order (element 4, then 1); with a
/// filter, the filter's elements. Element equations come from the group
/// tuple of each node (upstream indexes the equation numbers by node id
/// here, out of bounds for this group: UD-441, A).
#[test]
fn g02_matrix_assembly_is_restricted_to_the_group_elements() {
    let mesh = bar_with_group();
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_nodal_dofs_on_group(&mesh, "u", 2, "g").unwrap();
    dm.new_matrix("K", MatrixType::Unsymmetric).unwrap();
    let mut el = Array::new(16);
    el.push_tuple(&element_matrix(0));
    el.push_tuple(&element_matrix(1));
    dm.assemble_elemental_matrices_to_matrix(
        &mesh,
        "K",
        "u",
        el.view(),
        SEG,
        GhostType::NotGhost,
        MatrixType::Unsymmetric,
        None,
    )
    .unwrap();
    // element 4: nodes (4, 5) = tuples (0, 1) → equations 0..4;
    // element 1: nodes (1, 2) = tuples (2, 3) → equations 4..8
    let mut want = BTreeMap::new();
    for (k, first) in [(0usize, 0 as GlobalIdx), (1, 4)] {
        let m = element_matrix(k);
        for j in 0..4 {
            for i in 0..4 {
                want.insert(
                    (first + i as GlobalIdx, first + j as GlobalIdx),
                    m[i + 4 * j],
                );
            }
        }
    }
    assert_eq!(entries(&dm, "K"), want);

    // a filter wins over the group: element 1 alone, added again
    let mut one = Array::new(16);
    one.push_tuple(&element_matrix(2));
    dm.assemble_elemental_matrices_to_matrix(
        &mesh,
        "K",
        "u",
        one.view(),
        SEG,
        GhostType::NotGhost,
        MatrixType::Unsymmetric,
        Some(&[1]),
    )
    .unwrap();
    let m2 = element_matrix(2);
    for j in 0..4 {
        for i in 0..4 {
            *want
                .get_mut(&(4 + i as GlobalIdx, 4 + j as GlobalIdx))
                .unwrap() += m2[i + 4 * j];
        }
    }
    assert_eq!(entries(&dm, "K"), want);

    // an element with a node outside the group, and a wrong number of
    // element matrices, are errors that leave K unchanged
    let before = entries(&dm, "K");
    let mut bad = Array::new(16);
    bad.push_tuple(&element_matrix(3));
    for filter in [Some(&[0 as Idx][..]), Some(&[2][..]), None] {
        assert!(
            dm.assemble_elemental_matrices_to_matrix(
                &mesh,
                "K",
                "u",
                bad.view(),
                SEG,
                GhostType::NotGhost,
                MatrixType::Unsymmetric,
                filter,
            )
            .is_err(),
            "{filter:?}"
        );
    }
    assert_eq!(entries(&dm, "K"), before);
}

/// `onNodesAdded`: a group-supported family grows only by the new nodes of
/// its group's node group (upstream `node_group.find(node) != -1`); a
/// whole-mesh family by every new node; families grow in lexicographic
/// order (upstream `std::map` of DOF ids).
#[test]
fn g02_nodes_added_are_filtered_by_the_support_group() {
    let mut mesh = bar_with_group();
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_nodal_dofs_on_group(&mesh, "u", 2, "g").unwrap();
    dm.register_nodal_dofs(&mesh, "w", 1).unwrap();
    let (n6, n7) = {
        let mut acc = MeshAccessor::new(&mut mesh);
        (
            acc.push_node(&[6.0]).unwrap(),
            acc.push_node(&[7.0]).unwrap(),
        )
    };
    mesh.groups_mut()
        .element_group_mut("g")
        .unwrap()
        .add_node(n6, true);
    dm.on_nodes_added(&mesh, &[n6, n7]).unwrap();

    let u = dm.family("u").unwrap();
    assert_eq!(u.dofs.len(), 5, "only node 6 belongs to the group");
    assert_eq!(u.node_to_tuple.as_ref().unwrap().get(&n6), Some(&4));
    assert!(!u.node_to_tuple.as_ref().unwrap().contains_key(&n7));
    assert_eq!(u.associated_nodes, vec![4, 4, 5, 5, 1, 1, 2, 2, 6, 6]);
    // "u" < "w": the group family's new equations come first
    assert_eq!(
        dm.local_equation_numbers("u").unwrap(),
        &[0, 1, 2, 3, 4, 5, 6, 7, 14, 15]
    );
    let w = dm.family("w").unwrap();
    assert_eq!(w.dofs.len(), 8);
    assert_eq!(w.associated_nodes, vec![0, 1, 2, 3, 4, 5, 6, 7]);
    assert_eq!(
        dm.local_equation_numbers("w").unwrap(),
        &[8, 9, 10, 11, 12, 13, 16, 17]
    );
    assert_eq!(dm.system_size(), 18);

    // new nodes outside the group only: the group family does not change
    let n8 = MeshAccessor::new(&mut mesh).push_node(&[8.0]).unwrap();
    dm.on_nodes_added(&mesh, &[n8]).unwrap();
    assert_eq!(dm.family("u").unwrap().dofs.len(), 5);
    assert_eq!(dm.family("w").unwrap().dofs.len(), 9);
    assert_eq!(dm.local_equation_numbers("w").unwrap().last(), Some(&18));
    assert_eq!(dm.system_size(), 19);
}

// ---------------------------------------------------------------

/// `assemblePreassembledMatrix` with two DOF families
/// (`dof_manager_inline_impl.hh:305-322`): term `(i, j)` of
/// `TermsToAssemble(m, n)` lands at `(global(eq_m(i)), global(eq_n(j)))`,
/// accumulated in term order; the generic family is numbered first here,
/// so the nodal family's equations are offset.
#[test]
fn g03_preassembled_terms_across_a_nodal_and_a_generic_family() {
    let mesh = serial_bar(2);
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_generic_dofs("lambda", 2, 1).unwrap();
    dm.register_nodal_dofs(&mesh, "u", 2).unwrap();
    assert_eq!(dm.local_equation_numbers("lambda").unwrap(), &[0, 1]);
    assert_eq!(dm.local_equation_numbers("u").unwrap(), &[2, 3, 4, 5, 6, 7]);
    dm.new_matrix("K", MatrixType::Unsymmetric).unwrap();

    // λ0 constrains u_x of node 0 (DOF 0 of "u"), λ1 constrains u_y of node
    // 2 (DOF 5) with weight -2.5; the constraint rows are the transposes
    let mut b = TermsToAssemble::new("u", "lambda");
    b.add(0, 0, 1.0);
    b.add(5, 1, -2.5);
    b.add(0, 0, 0.5); // same term: accumulated in the term (1.5)
    let mut bt = TermsToAssemble::new("lambda", "u");
    bt.add(0, 0, 1.5);
    bt.add(1, 5, -2.5);
    dm.assemble_preassembled_matrix("K", &b).unwrap();
    dm.assemble_preassembled_matrix("K", &bt).unwrap();
    let want: BTreeMap<(GlobalIdx, GlobalIdx), f64> =
        [((2, 0), 1.5), ((7, 1), -2.5), ((0, 2), 1.5), ((1, 7), -2.5)]
            .into_iter()
            .collect();
    assert_eq!(entries(&dm, "K"), want);

    // the same terms again add up (A.add)
    dm.assemble_preassembled_matrix("K", &b).unwrap();
    assert_eq!(entries(&dm, "K")[&(2, 0)], 3.0);
    assert_eq!(entries(&dm, "K")[&(7, 1)], -5.0);

    // a DOF index outside a family, or an unknown family, is an error that
    // leaves K unchanged
    let before = entries(&dm, "K");
    for (m, n, i, j) in [
        ("u", "lambda", 0, 2),
        ("u", "lambda", 6, 0),
        ("lambda", "u", -1, 0),
    ] {
        let mut t = TermsToAssemble::new(m, n);
        t.add(i, j, 1.0);
        assert!(
            dm.assemble_preassembled_matrix("K", &t).is_err(),
            "{m} {n} {i} {j}"
        );
    }
    let mut t = TermsToAssemble::new("u", "nope");
    t.add(0, 0, 1.0);
    assert!(dm.assemble_preassembled_matrix("K", &t).is_err());
    assert_eq!(entries(&dm, "K"), before);
}

// ---------------------------------------------------------------

/// Residual and lumped matrix of a fixed 4-DOF system.
struct LumpedPhysics {
    r: [Real; 4],
}

impl SolverCallback for LumpedPhysics {
    fn assemble_residual(&mut self, dofs: &mut DofManager) -> Result<()> {
        dofs.zero_residual();
        dofs.residual_mut().as_mut_slice().copy_from_slice(&self.r);
        Ok(())
    }
}

/// `NonLinearSolverLumped`: `x = b_a2x · (b / A)` per equation
/// (`non_linear_solver_lumped.cc:92-94`), with `b_a2x` parsed from the
/// solver's section (`registerParam("b_a2x", alpha, 1., _pat_parsmod)`) or
/// set afterwards; the solution is split into the family.
#[test]
fn g07_lumped_solver_uses_b_a2x() {
    let mesh = serial_bar(1);
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_nodal_dofs(&mesh, "u", 2).unwrap();
    let a = [2.0, 4.0, 3.0, 0.3];
    dm.new_lumped_matrix("M")
        .unwrap()
        .values
        .copy_from_slice(&a);
    let r = [3.0, -1.0, 7.0, 0.1];
    let mut physics = LumpedPhysics { r };

    let parser = Parser::parse_str(
        "non_linear_solver lumped [\n  b_a2x = 0.5\n]\n",
        "lumped.dat",
    )
    .unwrap();
    let section = parser
        .root
        .sub_sections_of(ParserType::NonLinearSolver)
        .next()
        .unwrap();
    let mut nls = NonLinearSolverLumped::new("lumped", NonLinearSolverType::Lumped).unwrap();
    assert_eq!(nls.get_param("b_a2x").unwrap(), ParamValue::Real(1.0));
    nls.parse_section(section, false).unwrap();
    assert_eq!(nls.get_param("b_a2x").unwrap(), ParamValue::Real(0.5));

    let check = |dm: &DofManager, alpha: Real| {
        let want: Vec<u64> = r
            .iter()
            .zip(&a)
            .map(|(b, a)| (alpha * (b / a)).to_bits())
            .collect();
        let got: Vec<u64> = dm.solution().iter().map(|x| x.to_bits()).collect();
        assert_eq!(got, want, "alpha = {alpha}");
        let split: Vec<u64> = dm
            .dofs_solution("u")
            .unwrap()
            .as_slice()
            .iter()
            .map(|x| x.to_bits())
            .collect();
        assert_eq!(split, want, "alpha = {alpha}: solution per DOF");
    };
    nls.solve(&mut dm, &mut physics).unwrap();
    check(&dm, 0.5);

    // modifiable after parsing (`_pat_parsmod`); alpha·(b/A), not
    // (alpha·b)/A: 3·(0.1/0.3) and (3·0.1)/0.3 differ in the last bit
    assert_ne!(
        (3.0f64 * (0.1 / 0.3)).to_bits(),
        ((3.0f64 * 0.1) / 0.3).to_bits()
    );
    nls.set_param("b_a2x", ParamValue::Real(3.0)).unwrap();
    nls.solve(&mut dm, &mut physics).unwrap();
    check(&dm, 3.0);
}

// ---------------------------------------------------------------

fn init_error(text: &str) -> String {
    let parser = Parser::parse_str(text, "solvers.dat").unwrap();
    let mut dm = DofManager::new("dm", Communicator::serial());
    let mut model = MyModel::new(1.0, gen_mesh(3), false, &mut dm).unwrap();
    let mut ms = ModelSolver::new(dm);
    match ms.init_from_section(
        &parser.root.sub_sections[0],
        &mut model,
        parser.is_permissive(),
    ) {
        Err(Error::InvalidArgument(m)) => m,
        other => panic!("expected an invalid-argument error, got {other:?}"),
    }
}

/// Error branches of `initDOFManager(section, type)` (`model_solver.cc:
/// 98-217`) with upstream's messages.
#[test]
fn g08_init_from_section_error_branches_carry_upstream_messages() {
    assert_eq!(
        init_error(
            "model m [\n  time_step_solver static [\n    name = s1\n    \
             non_linear_solver newton_raphson [\n    ]\n    non_linear_solver linear [\n    ]\n  ]\n]\n"
        ),
        "More than one non linear solver are provided for the time step solver s1"
    );
    assert_eq!(
        init_error("model m [\n  default_solver = nothing\n]\n"),
        "The solver \"nothing\" was not created, it cannot be set as default solver"
    );
    assert_eq!(
        init_error(
            "model m [\n  time_step_solver static [\n    integration_scheme pseudo_time [\n    ]\n  ]\n]\n"
        ),
        "No degree of freedom name specified for the integration scheme of type pseudo_time"
    );
    // getOptionToType = exact operator>>: underscore spellings
    // and unknown names are rejected with the enumeration's message
    for (text, value, ty) in [
        (
            "model m [\n  time_step_solver _static [\n  ]\n]\n",
            "_static",
            "TimeStepSolverType",
        ),
        (
            "model m [\n  time_step_solver static [\n    sparse_solver_type = _eigen\n  ]\n]\n",
            "_eigen",
            "SparseSolverType",
        ),
        (
            "model m [\n  time_step_solver static [\n    non_linear_solver newton [\n    ]\n  ]\n]\n",
            "newton",
            "NonLinearSolverType",
        ),
        (
            "model m [\n  time_step_solver static [\n    integration_scheme _pseudo_time [\n      name = disp\n    ]\n  ]\n]\n",
            "_pseudo_time",
            "IntegrationSchemeType",
        ),
    ] {
        let m = init_error(text);
        assert!(
            m.starts_with(&format!(
                "The value {value} is not a valid {ty} valid values are \""
            )),
            "{m}"
        );
    }
    // an unknown solution_type: upstream's operator>> sets failbit, which
    // ParserParameter::operator T ignores (only badbit is checked), leaving
    // the enumeration uninitialised (UD-442, A); the port refuses it
    let m = init_error(
        "model m [\n  time_step_solver static [\n    integration_scheme pseudo_time [\n      name = disp\n      solution_type = jerk\n    ]\n  ]\n]\n",
    );
    assert!(m.contains("jerk"), "{m}");
}

/// `initDOFManager(dof_manager_type)`: only the `default` DOF manager is
/// registered in this build; any other name fails with upstream's message,
/// which appends the factory's text without a separator.
#[test]
fn g08_unknown_dof_manager_type_is_refused_with_the_factory_message() {
    check_dof_manager_type("default").unwrap();
    let Err(Error::InvalidArgument(m)) = check_dof_manager_type("petsc_like") else {
        panic!("an unknown DOF manager type must be refused");
    };
    assert_eq!(
        m,
        "To use the solver petsc_like you will have to code it. This is an unknown solver \
         type.The id \"petsc_like\" is not registered in the akantu::DOFManager factory."
    );
    assert!(check_dof_manager_type("Default").is_err());
}

// ---------------------------------------------------------------

type Log = Arc<Mutex<Vec<String>>>;

/// Test double recording `destroy_internal_data` and its drop.
#[derive(Debug)]
struct ProbeSolver {
    name: &'static str,
    log: Log,
}

impl SparseDirectSolver for ProbeSolver {
    fn kind(&self) -> SparseSolverType {
        SparseSolverType::Eigen
    }
    fn capabilities(&self) -> BackendCapabilities {
        centralized_capabilities()
    }
    fn analysis(&mut self, _a: &SparseMatrixAij) -> Result<()> {
        Ok(())
    }
    fn factorize(&mut self, _a: &SparseMatrixAij) -> Result<()> {
        Ok(())
    }
    fn solve(&mut self, b: &[Real], x: &mut [Real]) -> Result<()> {
        x.copy_from_slice(b);
        Ok(())
    }
    fn destroy_internal_data(&mut self) {
        self.log
            .lock()
            .unwrap()
            .push(format!("destroy {}", self.name));
    }
}

impl Drop for ProbeSolver {
    fn drop(&mut self) {
        self.log.lock().unwrap().push(format!("drop {}", self.name));
    }
}

/// Upstream registers every `SparseSolver` with its communicator and
/// destroys its internal data on `onCommunicatorFinalize`. Here the DOF
/// manager owns the solvers and a handle of the communicator: when it is
/// dropped, every installed solver releases its internal data **before**
/// the communicator's finalisation runs (the DOF manager held the last
/// handle); a replaced solver releases it at once.
#[test]
fn g14_solvers_release_internal_data_before_the_communicator_finalizes() {
    let log: Log = Arc::default();
    let comm = Communicator::serial();
    {
        let l = Arc::clone(&log);
        comm.on_finalize(move || l.lock().unwrap().push("finalize".into()));
    }
    let mut dm = DofManager::new("dm", comm); // the last handle
    let probe = |name| {
        Box::new(ProbeSolver {
            name,
            log: Arc::clone(&log),
        })
    };
    dm.backend_mut()
        .unwrap()
        .set_sparse_solver("J", probe("a"))
        .unwrap();
    dm.backend_mut()
        .unwrap()
        .set_sparse_solver("K", probe("k"))
        .unwrap();
    assert!(log.lock().unwrap().is_empty());
    dm.backend_mut()
        .unwrap()
        .set_sparse_solver("J", probe("b"))
        .unwrap();
    assert_eq!(*log.lock().unwrap(), ["destroy a", "drop a"]);
    drop(dm);
    let events = log.lock().unwrap().clone();
    let at = |e: &str| {
        events
            .iter()
            .position(|x| x == e)
            .unwrap_or_else(|| panic!("{e} missing in {events:?}"))
    };
    assert_eq!(
        events.iter().filter(|e| *e == "finalize").count(),
        1,
        "{events:?}"
    );
    for name in ["b", "k"] {
        assert!(
            at(&format!("destroy {name}")) < at("finalize"),
            "{events:?}"
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| **e == format!("destroy {name}"))
                .count(),
            1,
            "{events:?}"
        );
        at(&format!("drop {name}"));
    }
}
