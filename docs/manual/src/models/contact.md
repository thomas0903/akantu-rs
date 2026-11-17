<!-- availability: available akantu::contact -->
# Contact mechanics

<!-- BEGIN GENERATED: availability-status contact -->
**Available in this release** (facade module `akantu::contact`, crate `akantu-contact`, feature `contact`, on by default).
<!-- END GENERATED: availability-status contact -->

Upstream: `ContactMechanicsModel` (`contactmechanicsmodel.rst`; parity
rows CONT-001…018, DOC-012). The port's model,
`akantu::contact::ContactMechanicsModel`, detects contact between a
master and a slave surface and computes penalty contact forces and
stiffnesses with Coulomb friction:

* surface selectors (`PhysicalSurfaceSelector` on the element groups
  named by the `master` and `slave` parameters of the `contact_detector`
  section, `CohesiveSurfaceSelector` on cohesive facets,
  `AllSurfaceSelector` for both);
* detection (`ContactDetector`, section `contact_detector`): a global
  search with bounding boxes and a spatial grid, then the closest master
  node, the orthogonal projection on the master facets and the gap,
  normal and natural coordinates of every slave node in contact;
* resolutions (sections `contact_resolution <type> [ … ]` of the
  model's section or of the root): the types registered by the model's
  factory are listed in [Materials](../materials.md#registered-laws)
  (`penalty_linear`, `penalty_quadratic`), with the penalties
  `epsilon_n`, `epsilon_t`, the friction coefficient `mu` and
  `is_master_deformable`.

This chapter describes the model on its own. The couplers
(solid–contact, solid–cohesive–contact) are in [Model couplers](couplers.md). The model is serial (a distributed mesh is
refused, see below), and only linear master facets (`_segment_2`,
`_triangle_3`, `_quadrangle_4`) are supported: quadratic facets make the
search return `Error::NotImplemented` where upstream throws (UD-373; test
`contact_fixtures.rs::contact_fixtures_adversarial_quadratic_master`).

## Theory

This section follows the theory of upstream's chapter and then states
the discrete formulation the port implements (the penalty resolutions of
`akantu::contact::resolution` and the detector of
`akantu::contact::detector`), with the conventions of its arrays.

### The two-body problem

![Two bodies in their reference configuration, their motion to the
current configuration, the gap between a point of one contact surface and
its projection on the other, and the compressive contact pressure where
they touch](../figures/upstream/contact_mechanics_schematic.png)
*Figure: two bodies $\Omega_1$, $\Omega_2$ with their potential contact
surfaces $\Gamma^C_\alpha$, the gap $g$ between a point $\boldsymbol{r}$
and its projection $\boldsymbol{\rho}$, and the contact pressure where
they touch (figure of the Akantu manual).*

Two deformable bodies $\Omega_\alpha$, $\alpha = 1, 2$, have boundaries
$\Gamma_\alpha$ divided into a part $\Gamma^D_\alpha$ with prescribed
displacements, a part $\Gamma^N_\alpha$ with prescribed tractions and a
part $\Gamma^C_\alpha$ where the two bodies may come into contact (the
*potential contact zone*):

$$
\Gamma^D_\alpha \cup \Gamma^N_\alpha \cup \Gamma^C_\alpha = \Gamma_\alpha,
\qquad
\Gamma^D_\alpha \cap \Gamma^N_\alpha \cap \Gamma^C_\alpha = \emptyset .
$$

The motion maps each body from its reference configuration (points
$\boldsymbol{X}_\alpha$) to its current configuration at time
$t \in [0, T]$ (points $\boldsymbol{x}_\alpha$). Two physical conditions
hold on $\Gamma^C_\alpha$: the surfaces do not interpenetrate, and the
bodies exert on each other tractions that resist the interpenetration and
the relative sliding. The contact tractions $\boldsymbol{T}_\alpha$ enter
as an extra boundary condition,
$\boldsymbol{T}\vert_{\Gamma^C_\alpha} = \boldsymbol{T}_\alpha$, and with
the solution spaces

$$
\mathcal{U}_\alpha = \{ \boldsymbol{u}_\alpha : \Omega_\alpha \to \mathbb{R}^d \mid \boldsymbol{u}_\alpha = \bar{\boldsymbol{u}}_\alpha \text{ on } \Gamma^D_\alpha \},
\qquad
\mathcal{W}_\alpha = \{ \boldsymbol{w}_\alpha : \Omega_\alpha \to \mathbb{R}^d \mid \boldsymbol{w}_\alpha = 0 \text{ on } \Gamma^D_\alpha \}
$$

the weak form of each body is

$$
\int_{\Omega_\alpha} \boldsymbol{\sigma}[\boldsymbol{u}_\alpha] : \boldsymbol{\varepsilon}[\boldsymbol{w}_\alpha] \,\mathrm{d}\Omega
= \int_{\Omega_\alpha} \boldsymbol{b}_\alpha \cdot \boldsymbol{w}_\alpha \,\mathrm{d}\Omega
+ \int_{\Gamma^C_\alpha} \boldsymbol{T}_\alpha \cdot \boldsymbol{w}_\alpha \,\mathrm{d}\Gamma
+ \int_{\Gamma^N_\alpha} \boldsymbol{T}^D_\alpha \cdot \boldsymbol{w}_\alpha \,\mathrm{d}\Gamma .
$$

By the balance of momentum across the interface the tractions are equal
and opposite, $\boldsymbol{T}_1 \,\mathrm{d}\Gamma^C_1 = -\boldsymbol{T}_2 \,\mathrm{d}\Gamma^C_2$,
so the two contact integrals reduce to one integral over the surface of
body 1, the *slave* surface (the other one is the *master* surface):

$$
\sum_{\alpha=1}^{2} \int_{\Gamma^C_\alpha} \boldsymbol{T}_\alpha \cdot \delta\boldsymbol{u}_\alpha \,\mathrm{d}\Gamma
= \int_{\Gamma^C_1} \boldsymbol{T}_1 \cdot (\delta\boldsymbol{u}_1 - \delta\boldsymbol{u}_2) \,\mathrm{d}\Gamma .
$$

### The gap function and the closest-point projection

The non-penetration condition is written with a *gap function*. For a
point $\boldsymbol{r}$ of the slave surface, $\boldsymbol{\rho}$ is its
closest-point projection on the master surface and $\boldsymbol{n}$ the
outward normal of the master surface at $\boldsymbol{\rho}$:

$$
g = (\boldsymbol{r} - \boldsymbol{\rho}) \cdot \boldsymbol{n} .
$$

In this form $g > 0$ is an open gap, $g = 0$ contact and $g < 0$
interpenetration. On a master facet with nodes $\boldsymbol{x}_I$ and
shape functions $N_I(\boldsymbol{\xi})$, the projection is the point
$\boldsymbol{x}(\boldsymbol{\xi}) = \sum_I N_I(\boldsymbol{\xi}) \boldsymbol{x}_I$
that minimises the distance to the slave node $\boldsymbol{s}$. The
detector finds the natural coordinates $\boldsymbol{\xi}$ (there are
$d - 1$ of them) by Newton iterations on the stationarity condition

$$
f_\alpha(\boldsymbol{\xi}) = -2 \, \frac{\partial \boldsymbol{x}}{\partial \xi_\alpha} \cdot \big(\boldsymbol{s} - \boldsymbol{x}(\boldsymbol{\xi})\big) = 0,
\qquad
J_{\alpha\beta} = 2 \left( a_{\alpha\beta} - \frac{\partial^2 \boldsymbol{x}}{\partial \xi_\alpha \partial \xi_\beta} \cdot \big(\boldsymbol{s} - \boldsymbol{x}(\boldsymbol{\xi})\big) \right),
$$

starting from $\boldsymbol{\xi} = \boldsymbol{0}$, with the surface metric
$a_{\alpha\beta} = \frac{\partial \boldsymbol{x}}{\partial \xi_\alpha} \cdot \frac{\partial \boldsymbol{x}}{\partial \xi_\beta}$,
until $\lVert \boldsymbol{f} \rVert$ is at most `projection_tolerance` or
`max_iterations` iterations were made (upstream's metric differs, UD-363,
below). The second derivatives $\partial^2 N_I / \partial \xi_\alpha \partial \xi_\beta$
are what restricts the master facets to the linear types. At the
projection, the facet's unit tangents
$\boldsymbol{\tau}_\alpha = \frac{\partial \boldsymbol{x}}{\partial \xi_\alpha} \big/ \big\lVert \frac{\partial \boldsymbol{x}}{\partial \xi_\alpha} \big\rVert$
give the outward unit normal $\boldsymbol{n}$, oriented away from the
master body. A facet is a candidate when the slave lies on the inner side
of it, $(\boldsymbol{s} - \boldsymbol{x}(\boldsymbol{\xi})) \cdot \boldsymbol{n} < 0$
(interpenetration), and when its projection lies in the facet extended by
`extension_tolerance`; among the candidates the facet with the smallest
distance $\lVert \boldsymbol{s} - \boldsymbol{x}(\boldsymbol{\xi}) \rVert$
wins.

The port stores the opposite sign: the model's `gaps` array holds the
interpenetration $\lVert \boldsymbol{s} - \boldsymbol{x}(\boldsymbol{\xi}) \rVert$,
positive for a slave node that has penetrated the master surface (and
zero for a node with no contact element), as upstream's `gaps` array.
Only penetrating slave nodes get a contact element; there is no search
margin. In the formulas of the resolutions below, $g$ is this stored,
positive interpenetration.

![A slave node s below a master segment between master nodes 1 and 2,
its projection x(ξ) on the segment, the gap g between them, the outward
normal n and the unit tangent τ of the segment, and the contact force on
the slave node along n](../figures/contact-node-to-segment.svg)
*Figure: a node-to-segment contact element: the slave node $s$ has
penetrated the master segment by $g$; its projection
$\boldsymbol{x}(\boldsymbol{\xi})$ carries the outward normal
$\boldsymbol{n}$ and the unit tangent $\boldsymbol{\tau}$, and the normal
contact force pushes $s$ out along $\boldsymbol{n}$.*

### The contact constraints

The contact traction splits into a normal and a tangential part,

$$
\boldsymbol{T}_1 = \boldsymbol{T}^n + \boldsymbol{T}^t = \sigma_n \boldsymbol{n} + \boldsymbol{T}^t ,
$$

where the normal pressure $\sigma_n$ is zero away from contact and
compressive in contact. With the gap of the theory ($g \geq 0$ open),
these are the *Hertz–Signorini–Moreau* conditions, also known as the
Karush–Kuhn–Tucker conditions of the normal contact:

$$
g \geq 0, \qquad \sigma_n \leq 0, \qquad \sigma_n \, g = 0 .
$$

In the tangential direction, the friction traction acts against the
relative sliding velocity $\boldsymbol{v}_t$ of $\boldsymbol{r}$ with
respect to $\boldsymbol{\rho}$, in the direction

$$
\boldsymbol{s} =
\begin{cases}
\dfrac{\boldsymbol{v}_t}{\lVert \boldsymbol{v}_t \rVert}, & \lVert \boldsymbol{v}_t \rVert > 0, \\
\boldsymbol{0}, & \lVert \boldsymbol{v}_t \rVert = 0 .
\end{cases}
$$

The model implements Coulomb's law (Coulomb, 1821): the frictional
strength is proportional to the normal pressure,
$\sigma_\text{fric} = \mu \lvert \sigma_n \rvert$ with the friction
coefficient $\mu$ (`mu`). Below the strength the surfaces stick, at the
strength they slip:

$$
\begin{aligned}
&\lVert \boldsymbol{T}^t \rVert < \mu \lvert \sigma_n \rvert, && \boldsymbol{v}_t = \boldsymbol{0} && \text{(stick)},\\
&\lVert \boldsymbol{T}^t \rVert - \mu \lvert \sigma_n \rvert = 0, && \lVert \boldsymbol{v}_t \rVert > 0 && \text{(slip)},
\end{aligned}
$$

which together form the complementarity conditions of the tangential
contact:

$$
\lVert \boldsymbol{v}_t \rVert \geq 0, \qquad
\lVert \boldsymbol{T}^t \rVert - \mu \lvert \sigma_n \rvert \leq 0, \qquad
\lVert \boldsymbol{v}_t \rVert \big( \lVert \boldsymbol{T}^t \rVert - \mu \lvert \sigma_n \rvert \big) = 0 .
$$

These constraints are unilateral, so the balance of virtual work becomes
a variational inequality,

$$
\sum_{\alpha=1}^{2} \int_{\Omega_\alpha} \boldsymbol{\sigma}[\boldsymbol{u}_\alpha] : \boldsymbol{\varepsilon}[\boldsymbol{w}_\alpha] \,\mathrm{d}\Omega
\geq \sum_{\alpha=1}^{2} \left\{ \int_{\Omega_\alpha} \boldsymbol{b}_\alpha \cdot \boldsymbol{w}_\alpha \,\mathrm{d}\Omega
+ \int_{\Gamma^C_\alpha} \boldsymbol{T}_\alpha \cdot \boldsymbol{w}_\alpha \,\mathrm{d}\Gamma
+ \int_{\Gamma^N_\alpha} \boldsymbol{T}^D_\alpha \cdot \boldsymbol{w}_\alpha \,\mathrm{d}\Gamma \right\},
$$

a non-linear optimisation problem. The finite-element solution uses an
*active set*: the bodies are first allowed to interpenetrate, the
penetrated part $\Gamma^{C\star}_1 \subseteq \Gamma^C_1$ of the slave
surface is found (the contact detection), and the constraints are
imposed on that part only, which turns the inequality into an equality:

$$
\begin{aligned}
\sum_{\alpha=1}^{2} \int_{\Omega_\alpha} \boldsymbol{\sigma}[\boldsymbol{u}_\alpha] : \boldsymbol{\varepsilon}[\delta\boldsymbol{u}_\alpha] \,\mathrm{d}\Omega
={}& \sum_{\alpha=1}^{2} \left\{ \int_{\Omega_\alpha} \boldsymbol{b}_\alpha \cdot \delta\boldsymbol{u}_\alpha \,\mathrm{d}\Omega
+ \int_{\Gamma^N_\alpha} \boldsymbol{T}^D_\alpha \cdot \delta\boldsymbol{u}_\alpha \,\mathrm{d}\Gamma \right\} \\
&+ \int_{\Gamma^{C\star}_1} \boldsymbol{T}_1 \cdot (\delta\boldsymbol{u}_1 - \delta\boldsymbol{u}_2) \,\mathrm{d}\Gamma .
\end{aligned}
$$

A contact computation therefore has two steps at every residual: the
detection of the active set (`ContactMechanicsModel::search`) and the
computation of the contact forces on it
(`ContactMechanicsModel::assemble_internal_forces`).

### The penalty formulation

The resolutions `penalty_linear` and `penalty_quadratic` impose the
normal constraint by a penalty: instead of the exact conditions above, a
pressure that grows with the interpenetration $g$ (the stored, positive
value) pushes the surfaces apart,

$$
p_N = \epsilon_n \langle g \rangle_+ \quad (\texttt{penalty\_linear}),
\qquad
p_N = \epsilon_n \big( \langle g \rangle_+^2 + \langle g \rangle_+ \big) \quad (\texttt{penalty\_quadratic}),
$$

with the normal penalty $\epsilon_n$ (`epsilon_n`) and the Macaulay
bracket $\langle g \rangle_+ = \max(g, 0)$. The contact term of the weak
form becomes

$$
\delta W_c = \int_{\Gamma^{C\star}_1} \big( p_N \boldsymbol{n} + \boldsymbol{t}_T \big) \cdot (\delta\boldsymbol{u}_1 - \delta\boldsymbol{u}_2) \,\mathrm{d}\Gamma ,
$$

with $\boldsymbol{t}_T$ the friction traction below. The penalty method
leaves a penetration of order $p_N / \epsilon_n$ in equilibrium: the
constraint is met exactly only in the limit
$\epsilon_n \to \infty$, while a large $\epsilon_n$ makes the stiffness
ill-conditioned and lowers the stable time step of an explicit
computation, so the penalty is a compromise chosen for each problem.

### The node-to-segment discretisation

The contact integral is evaluated at the slave nodes: each slave node
$s$ in contact and the master facet it projects on form a *contact
element* (`ContactElement`, with its `slave` node and `master` facet),
weighted by the nodal area $A_s$ of the slave node (the model's
`nodal_area` array). $A_s$ is the norm of
$\sum_e \int_{\Gamma_e} N_s \boldsymbol{n}_e \,\mathrm{d}\Gamma$ over the
facets $e$ of the contact surface around $s$, integrated on the current
positions; on a straight 2-D surface it is the sum of the halves of the
edges around the node. With the master facet's shape functions evaluated at the
projection, the matrix

$$
\boldsymbol{A} = \begin{bmatrix} \boldsymbol{I} & -N_1(\boldsymbol{\xi}) \boldsymbol{I} & \cdots & -N_n(\boldsymbol{\xi}) \boldsymbol{I} \end{bmatrix}
\qquad (d \times d(n+1))
$$

maps the displacements $(\boldsymbol{u}_s, \boldsymbol{u}_1, \ldots, \boldsymbol{u}_n)$
of the slave node and the $n$ master nodes to the relative displacement
of the slave node with respect to its projection. The normal force of
the contact element on these $n + 1$ nodes is

$$
\boldsymbol{f}_N = A_s \, p_N \, \boldsymbol{A}^T \boldsymbol{n} ,
$$

that is $A_s p_N \boldsymbol{n}$ on the slave node and
$-A_s p_N N_I(\boldsymbol{\xi}) \boldsymbol{n}$ on master node $I$. The
model adds the normal and the tangential forces into its
`internal_force` array (also `normal_force` and `tangential_force`
separately), which the couplers add to the solid's residual.

With `is_master_deformable = false` (a rigid master) only the slave
node's force is assembled, which saves the master nodes' work. With
`is_master_deformable = true` every node of the contact element receives
its force times the weight of `ContactMechanicsModel::deformable_master_weighting`:
$\tfrac12$ by default (`DeformableMasterWeighting::TwoPassHalf`, intended
for a symmetric selection where each surface is in turn master and
slave), or $1$ (`DeformableMasterWeighting::OnePassFull`). The stiffness
of a static or implicit computation is, by default, the consistent
tangent $-\partial \boldsymbol{f} / \partial \boldsymbol{u}$ of these
forces with the nodal area held fixed, weighted the same way (UD-002,
UD-368; upstream assigns the nodal forces and uses other moduli, below).

### Coulomb friction with stick and slip

When $\mu \neq 0$ and the slave node was already in contact at the
previous search, the resolution computes a friction traction by a return
map in the covariant components $t_\alpha$ of the tangent plane, with the
metric $a_{\alpha\beta} = \boldsymbol{\tau}_\alpha \cdot \boldsymbol{\tau}_\beta$
of the unit tangents and its inverse $a^{\alpha\beta}$:

1. The slip increment is the move of the projection since the previous
   search: with $\Delta\boldsymbol{x}$ the difference between the
   current projection on the current master facet and the previous
   projection on the previous master facet (both on the current
   positions),
   $\Delta\xi_\beta = \sum_\gamma a^{\beta\gamma} \, \Delta\boldsymbol{x} \cdot \boldsymbol{\tau}_\gamma$.
2. The trial traction is the previous traction, carried over from the
   previous tangents $\bar{\boldsymbol{\tau}}_\beta$ (metric inverse
   $\bar a^{\gamma\beta}$) to the current ones, minus the elastic
   response to the slip increment with the tangential penalty
   $\epsilon_t$ (`epsilon_t`):
   $$
   t^\text{trial}_\alpha = \sum_{\beta,\gamma} \bar t_\gamma \, \bar a^{\gamma\beta} \, (\bar{\boldsymbol{\tau}}_\beta \cdot \boldsymbol{\tau}_\alpha) - \epsilon_t \sum_\beta a_{\alpha\beta} \, \Delta\xi_\beta .
   $$
3. With the norm
   $\lVert \boldsymbol{t} \rVert = \sqrt{\sum_{\alpha,\beta} t_\alpha t_\beta a^{\alpha\beta}}$,
   the node sticks or slips:
   $$
   \boldsymbol{t} =
   \begin{cases}
   \boldsymbol{t}^\text{trial}, & \lVert \boldsymbol{t}^\text{trial} \rVert \leq \mu \, p_N \quad \text{(stick)},\\
   \mu \, p_N \, \dfrac{\boldsymbol{t}^\text{trial}}{\lVert \boldsymbol{t}^\text{trial} \rVert}, & \text{otherwise} \quad \text{(slip)}.
   \end{cases}
   $$
4. The tangential force of the contact element is
   $$
   \boldsymbol{f}_T = A_s \sum_{\alpha,\beta} a^{\alpha\beta} \, t_\beta \, \boldsymbol{A}^T \boldsymbol{\tau}_\alpha .
   $$

The traction and the state (stick or slip) of each slave node are the
model's `tangential_tractions` and `contact_state` arrays; the projections,
tangents, tractions and master facets of the previous search are kept as
the friction history. A slave node with no previous master facet (its
first search in contact) gets no friction force; its history then starts
from a zero traction. The formulas are the covariant ones of Konyukhov
and Schweizerhof (2004, 2005), which upstream's code cites.

## Parameters of the input file

The `contact_detector` section (read by `ContactDetector` and by the
surface selectors):

| Keyword | Meaning |
|---|---|
| `type` | `explicit` or `implicit`; required, and anything else is refused, but the search is the same for both (upstream stores a value that nothing reads) |
| `master` | the element group of the master surface (`PhysicalSurfaceSelector`) |
| `slave` | the element group of the slave surface (`PhysicalSurfaceSelector`) |
| `projection_tolerance` | the Newton tolerance on $\lVert \boldsymbol{f} \rVert$ of the closest-point projection; required |
| `max_iterations` | the largest number of Newton iterations of the projection; required |
| `extension_tolerance` | how far outside its facet a projection is accepted, in natural coordinates (the reference box $[-1 - \text{tol}, 1 + \text{tol}]^{d-1}$, or the reference triangle extended by it); required |

The search itself (`ContactDetector`) runs in two stages: a global
search keeps the master and slave nodes inside the intersection of the
two surfaces' bounding boxes (each enlarged by the largest in-radius of
the master facets) and puts the master nodes in a spatial grid; a local
search finds, for every slave node, the closest master node in the
neighbouring grid cells, and projects the slave node on the
`contact_surface` facets around that master node as described above.

The `contact_resolution <type>` sections (read by the resolutions; the
defaults are those of `ResolutionPenalty::new`):

| Keyword | Meaning |
|---|---|
| `name` | the name of the resolution |
| `epsilon_n` | the normal penalty $\epsilon_n$; default 0 |
| `epsilon_t` | the tangential penalty $\epsilon_t$ of the stick state; default 0 |
| `mu` | the Coulomb friction coefficient $\mu$; default 0 (no friction) |
| `is_master_deformable` | whether the master nodes receive forces; default `false` |

## A detection and a penalty force

Two blocks touch along $y = 0$: the lower block's top edge is the master
surface, the upper block's bottom edge the slave surface. The upper block
is moved down by $\delta$, so both slave nodes penetrate the master edge by
$\delta$; with `penalty_linear`, a rigid master (`is_master_deformable =
false`) and no friction, each slave node gets the force
$\epsilon_n \delta A \boldsymbol{n}$ ($A$ its nodal area on the slave
surface, $\boldsymbol{n}$ the master normal):

```rust
use akantu::contact::{ContactMechanicsModel, PhysicalSurfaceSelector};
use akantu::core::enums::AnalysisMethod;
use akantu::core::{Element, ElementType, GhostType};
use akantu::mesh::utils::fill_element_to_subelements_data;
use akantu::mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu::parser::Parser;

// lower block [−1, 1] × [−1, 0], upper block [−0.5, 0.5] × [0, 1]
let (q, s, g) = (ElementType::Quadrangle4, ElementType::Segment2, GhostType::NotGhost);
let mut mesh = Mesh::new(2, "blocks")?;
{
    let mut a = MeshAccessor::new(&mut mesh);
    for x in [[-1.0, -1.0], [1.0, -1.0], [1.0, 0.0], [-1.0, 0.0]] {
        a.push_node(&x)?; // nodes 0…3
    }
    for x in [[-0.5, 0.0], [0.5, 0.0], [0.5, 1.0], [-0.5, 1.0]] {
        a.push_node(&x)?; // nodes 4…7
    }
    // write guards on the connectivities, released at the end of the block
    let mut quads = a.connectivity_mut(q, g);
    quads.push_tuple(&[0, 1, 2, 3]);
    quads.push_tuple(&[4, 5, 6, 7]);
    drop(quads);
    let mut segments = a.connectivity_mut(s, g);
    segments.push_tuple(&[2, 3]); // master: top edge of the lower block
    segments.push_tuple(&[4, 5]); // slave: bottom edge of the upper block
}
for (name, el) in [("master", 0), ("slave", 1)] {
    mesh.groups_mut().create_element_group(name, 1)?;
    mesh.element_group_add(name, Element::new(s, el, g), true, true)?;
}
fill_element_to_subelements_data(&mut mesh)?;

let eps_n = 1e6;
let input = format!(
    "contact_detector [
        type = explicit
        master = master
        slave = slave
        projection_tolerance = 1e-10
        max_iterations = 100
        extension_tolerance = 1e-5
    ]
    contact_resolution penalty_linear [
        name = contact
        mu = 0
        epsilon_n = {eps_n}
        epsilon_t = 0
        is_master_deformable = false
    ]"
);
let parser = Parser::parse_str(&input, "contact.dat")?;
let selector = PhysicalSurfaceSelector::new(&mesh, &parser)?;
let mesh = SharedMesh::new(mesh);
let mut model = ContactMechanicsModel::with_parser(mesh.clone(), None, "contact", parser)?;
model.init_full(AnalysisMethod::ExplicitLumpedMass)?; // reads the resolutions
model.set_surface_selector(Box::new(selector))?;
assert_eq!(model.resolutions().len(), 1);
// the selector's surface is mirrored as the mesh group `contact_surface`
// (upstream's selectors create it; a contact model alone on its mesh)
assert_eq!(mesh.read()?.groups().element_group("contact_surface")?.nb_nodes(), 4);

// move the upper block down by δ (the model's current positions)
let delta = 1e-3;
for n in 4..8 {
    model.positions_mut().tuple_mut(n)[1] -= delta;
}
model.search()?;
assert_eq!(model.contact_elements().len(), 2); // the two slave nodes
for ce in model.contact_elements() {
    let n = ce.slave as usize;
    assert!((model.gaps().tuple(n)[0] - delta).abs() <= 1e-12 * delta);
    assert_eq!(model.normals().tuple(n), &[0.0, 1.0]); // out of the master
    assert_eq!(model.nodal_area().tuple(n)[0], 0.5); // half the slave edge
}
model.assemble_internal_forces()?;
for ce in model.contact_elements() {
    let forces = model.internal_force(); // a read guard on the shared array
    let f = forces.tuple(ce.slave as usize);
    let expected = eps_n * delta * 0.5;
    assert!(f[0].abs() <= 1e-12 * expected);
    assert!((f[1] - expected).abs() <= 1e-12 * expected);
}
# Ok::<(), akantu::core::Error>(())
```

## Differences from upstream

The corrected defaults and their switches (`ContactUpstreamCompat`;
`ContactUpstreamCompat::all()` reproduces upstream wherever upstream is
defined) are listed in the crate documentation of `akantu::contact` and in
[`docs/UPSTREAM_DEFECTS.md`](../../../UPSTREAM_DEFECTS.md) (UD-001,
UD-002, UD-008, UD-362…369, UD-371, UD-372). By default the normal and
friction tangents are consistent (UD-368, UD-369), the normals are
oriented on the current configuration (UD-371) and a projection is valid
when it lies in its facet (UD-372). A selector owns its contact surface
and mirrors it as the mesh's element group `contact_surface` when it is
set, as upstream's selectors create that group (the example above checks
it): `set_surface_selector` of a contact model alone on its mesh, of
`CouplerSolidContact` or of `CouplerSolidCohesiveContact` writes it, and
the solid–cohesive–contact coupler rewrites it in the facet mesh after
each insertion. The remaining observable differences
are listed in `akantu::contact::selector`.

The detection grid keeps upstream's cell edge, `√2 ·` the largest
in-radius of every facet around a master node (side facets included). With
side facets much longer than the contact-surface facets the cells become
large and a search slows down towards `O(slaves × masters)` (UD-1297).
`ContactMechanicsModel::set_grid_spacing(GridSpacing::ContactSurface)` (or
`ContactDetector::grid_spacing`) takes the contact-surface facets only; the
neighbourhood of a slave then covers another region, so the closest master
node found can differ from upstream's when it lies farther than one cell.

Contact mechanics on a distributed mesh is not part of this release and
is not planned (upstream has no distributed support for it either):
`init_full` refuses a mesh distributed over several processes with
`Error::Unsupported` (test
`manual_claims_2.rs::doc012_contact_model_refuses_a_distributed_mesh`).

Tests: `crates/akantu-contact/tests/`, ports of upstream's contact tests
(`contact_model.rs`: `test_explicit_resolution`, resolution
instantiation; `contact_detection.rs`: `test_detection` with analytical
gaps, normals and nodal areas), the friction tangents
(`contact_friction_tangent.rs`), the stiffness (`contact_stiffness.rs`,
finite differences) and the oracle replays of parity rows CONT-*
(`contact_fixtures.rs`, `contact_explicit.rs`); the availability of the
model and its resolution names:
`manual_claims_2.rs::doc012_contact_model_is_available`.

## References

* Coulomb, C. A. (1821). *Théorie des machines simples en ayant égard au
  frottement de leurs parties et à la roideur des cordages*. Bachelier.
* Konyukhov, A. and Schweizerhof, K. (2004, 2005). The two papers of
  *Computational Mechanics* cited by upstream's penalty resolutions:
  DOI 10.1007/s00466-003-0515-3 and DOI 10.1007/s00466-004-0616-7.
