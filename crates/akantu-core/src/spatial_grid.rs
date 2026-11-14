// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_grid_dynamic.hh
// (SpatialGrid<T>, CellID, neighbor_cells_iterator, Cell, saveAsMesh).

//! Dynamic spatial hash grid (upstream `SpatialGrid<T>`, CORE-030).
//!
//! Cells are addressed by integer ids `floor((x_d − center_d) / spacing_d)`
//! per direction and stored sparsely in a [`BTreeMap`] keyed by
//! [`CellId`] — lexicographic order, exactly upstream's
//! `std::map<CellID, Cell>`, so iteration over cells (and therefore every
//! output built from it: neighbour lists, grid synchronisation, the mesh of
//! [`SpatialGrid::mesh_arrays`]) follows upstream's order. Items in a cell
//! keep their insertion order. Neighbour cells ([`CellId::neighbors`]) are
//! the `3^d` cells with offsets in `{−1, 0, 1}^d`, **first coordinate
//! fastest**, the cell itself included (upstream base-3 counter).
//!
//! The grid lives in `akantu-core` (decision record D-048; not
//! `akantu-geometry` as first planned in ARCHITECTURE) because its
//! users sit on both sides of the crate graph: non-local neighbourhoods
//! (`akantu-model`), the grid synchroniser (`akantu-mesh`) and contact
//! detection (`akantu-contact`). It is the **only** spatial grid of the
//! port; `akantu-geometry` re-exports it. Upstream's `saveAsMesh` needs a
//! `Mesh`; its data is produced here by [`SpatialGrid::mesh_arrays`] and
//! written into a mesh by `akantu_geometry::save_grid_as_mesh`.
//!
//! What the users need, as upstream uses it:
//!
//! | Upstream | Here |
//! |---|---|
//! | `SpatialGrid(dim, spacing, center)`, `insert(d, position)` | [`SpatialGrid::with_spacing`], [`SpatialGrid::insert`] |
//! | `getCellID(position)`, `getCell(id)`, `getCell(position)` | [`SpatialGrid::cell_id`], [`SpatialGrid::cell`], [`SpatialGrid::cell_at`] |
//! | `getCellID(Real, Int)` (private; `GridSynchronizer` is a `friend`) | [`SpatialGrid::cell_index`] (public) |
//! | `CellID(dim)`, `setID`, `getID`, `getIDs` | [`CellId::new`], [`CellId::set_id`], [`CellId::id`], [`CellId::ids`] |
//! | `GridSynchronizer`'s loops over a range of ids + `getCell` | [`SpatialGrid::cells_in_range`] (same visited sequence) |
//! | `cell_id.neighbors()` | [`CellId::neighbors`] |
//! | `for (auto && id : grid)` | [`SpatialGrid::cells`], [`SpatialGrid::cell_ids`] |
//! | `getLowerBounds`, `getUpperBounds`, `getSpacing`, `getCenter` | [`SpatialGrid::lower_bounds`], [`SpatialGrid::upper_bounds`], [`SpatialGrid::spacing`], [`SpatialGrid::center`] |
//!
//! Positions must have exactly `dimension()` coordinates (upstream reads
//! the first `dimension` entries of any longer vector; a position of
//! another length is refused here).
//!
//! Deviations (misuse only, UD-270): a non-finite cell coordinate or one
//! outside `(Idx::MIN, Idx::MAX)` (upstream: undefined conversion of
//! `std::floor` to `Int`) is an error; the neighbours of a cell next to
//! that range are not yielded when they fall outside it (upstream: signed
//! overflow); spacing and center are zero until set
//! (upstream: uninitialised Eigen vectors); dimensions other than 1, 2, 3
//! cannot be saved as a mesh (upstream: `_not_defined` element type).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::element_type::ElementType;
use crate::error::{Error, Result};
use crate::random::fmt_c::format_g;
use crate::types::{Idx, Int, Real};

/// `true` for a cell id strictly inside `(Idx::MIN, Idx::MAX)`, the ids a
/// [`CellId`] may hold (so that `id ± 1` never overflows).
#[inline]
fn is_admissible(id: Idx) -> bool {
    id != Idx::MIN && id != Idx::MAX
}

/// Integer coordinates of a grid cell (upstream `SpatialGrid::CellID`);
/// ordered lexicographically (upstream `operator<`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CellId {
    ids: Vec<Idx>,
}

impl CellId {
    /// Cell with the given per-direction ids.
    ///
    /// # Errors
    /// [`Error::InvalidIndex`] if an id is `Idx::MIN` or `Idx::MAX` (their
    /// neighbours would overflow).
    pub fn new(ids: Vec<Idx>) -> Result<Self> {
        if let Some(&bad) = ids.iter().find(|&&i| !is_admissible(i)) {
            return Err(Error::InvalidIndex {
                what: "grid cell id",
                value: i64::from(bad),
            });
        }
        Ok(Self { ids })
    }

    /// Ids per direction (upstream `getIDs`).
    #[must_use]
    pub fn ids(&self) -> &[Idx] {
        &self.ids
    }

    /// Id in direction `dir` (upstream `getID`), `None` if out of range.
    #[must_use]
    pub fn id(&self, dir: usize) -> Option<Idx> {
        self.ids.get(dir).copied()
    }

    /// Set the id in direction `dir` (upstream `setID(dir, id)`, used by
    /// `GridSynchronizer` to enumerate a range of cells).
    ///
    /// # Errors
    /// [`Error::OutOfRange`] if `dir ≥ dim()` (upstream: unchecked Eigen
    /// access); [`Error::InvalidIndex`] for `Idx::MIN` / `Idx::MAX` (see
    /// [`CellId::new`]). The id is unchanged on error.
    pub fn set_id(&mut self, dir: usize, id: Idx) -> Result<()> {
        let len = self.ids.len();
        let slot = self.ids.get_mut(dir).ok_or(Error::OutOfRange {
            what: "grid cell id direction",
            index: dir,
            len,
        })?;
        if !is_admissible(id) {
            return Err(Error::InvalidIndex {
                what: "grid cell id",
                value: i64::from(id),
            });
        }
        *slot = id;
        Ok(())
    }

    /// Dimension of the cell id.
    #[must_use]
    pub fn dim(&self) -> usize {
        self.ids.len()
    }

    /// The `3^d` neighbouring cells, this cell included, in upstream's
    /// order: offsets in `{−1, 0, 1}^d`, first coordinate fastest
    /// (`neighbor_cells_iterator`).
    ///
    /// **Edge cells** (an id equal to `Idx::MIN + 1` or `Idx::MAX − 1`):
    /// the neighbours with an id equal to `Idx::MIN` or `Idx::MAX` are not
    /// yielded, so every yielded cell is admissible (see
    /// [`CellId::new`]) and the neighbours of a neighbour never overflow.
    /// No item is lost: a grid never holds such a cell
    /// ([`SpatialGrid::cell_id`] refuses it). Upstream computes those ids
    /// with signed `Int` overflow (undefined, UD-270).
    pub fn neighbors(&self) -> Neighbors<'_> {
        Neighbors {
            center: self,
            offset: vec![-1; self.ids.len()],
            done: false,
        }
    }
}

/// Iterator over the neighbouring cells of a [`CellId`].
#[derive(Debug, Clone)]
pub struct Neighbors<'a> {
    center: &'a CellId,
    offset: Vec<Idx>,
    done: bool,
}

impl Iterator for Neighbors<'_> {
    type Item = CellId;

    fn next(&mut self) -> Option<CellId> {
        while !self.done {
            // the centre's ids are admissible (strictly inside (MIN, MAX):
            // `CellId::new`, `SpatialGrid::cell_id` and this iterator only
            // build such ids), so `c + o` with `o ∈ {−1, 0, 1}` cannot
            // overflow
            let ids: Vec<Idx> = self
                .center
                .ids
                .iter()
                .zip(&self.offset)
                .map(|(c, o)| c + o)
                .collect();
            // upstream operator++: first direction not at +1 is
            // incremented, the directions before it go back to −1
            match self.offset.iter().position(|&o| o != 1) {
                Some(i) => {
                    for o in &mut self.offset[..i] {
                        *o = -1;
                    }
                    self.offset[i] += 1;
                }
                None => self.done = true,
            }
            // An edge cell's neighbours outside (MIN, MAX) are not
            // yielded (they can hold no item)
            if ids.iter().all(|&i| is_admissible(i)) {
                return Some(CellId { ids });
            }
        }
        None
    }
}

/// Reusable buffers of [`SpatialGrid::neighborhood_at`] (the centre cell,
/// the visited cell and the walk's offsets), so that walking the
/// neighbourhoods of many positions allocates only on the first walk.
#[derive(Clone, Debug)]
pub struct NeighborhoodScratch {
    center: Vec<Idx>,
    cell: CellId,
    offset: Vec<Idx>,
}

impl Default for NeighborhoodScratch {
    fn default() -> Self {
        Self {
            center: Vec::new(),
            cell: CellId { ids: Vec::new() },
            offset: Vec::new(),
        }
    }
}

/// Iterator of [`SpatialGrid::neighborhood_at`]: the items of each
/// neighbouring cell, in upstream's order.
#[derive(Debug)]
pub struct NeighborhoodItems<'a, 's, T> {
    grid: &'a SpatialGrid<T>,
    scratch: &'s mut NeighborhoodScratch,
    done: bool,
}

impl<'a, T> Iterator for NeighborhoodItems<'a, '_, T> {
    type Item = &'a [T];

    fn next(&mut self) -> Option<&'a [T]> {
        let grid: &'a SpatialGrid<T> = self.grid;
        let s = &mut *self.scratch;
        while !self.done {
            // the centre's ids are admissible (`SpatialGrid::cell_index`),
            // so `c + o` with `o ∈ {−1, 0, 1}` cannot overflow
            for ((id, c), o) in s.cell.ids.iter_mut().zip(&s.center).zip(&s.offset) {
                *id = c + o;
            }
            // the increment of `Neighbors::next` (upstream operator++)
            match s.offset.iter().position(|&o| o != 1) {
                Some(i) => {
                    for o in &mut s.offset[..i] {
                        *o = -1;
                    }
                    s.offset[i] += 1;
                }
                None => self.done = true,
            }
            // As `Neighbors::next`
            if s.cell.ids.iter().all(|&i| is_admissible(i)) {
                return Some(grid.cell(&s.cell));
            }
        }
        None
    }
}

/// Elements, nodes and tags of the mesh made of the grid's non-empty cells
/// (upstream `SpatialGrid::saveAsMesh`), see [`SpatialGrid::mesh_arrays`].
#[derive(Clone, Debug, PartialEq)]
pub struct GridMeshArrays {
    /// `Segment2`, `Quadrangle4` or `Hexahedron8` for dimension 1, 2, 3.
    pub element_type: ElementType,
    /// New node coordinates, `dimension` values per node, `2^d` nodes per
    /// cell (no node sharing between cells, as upstream).
    pub nodes: Vec<Real>,
    /// Connectivity of the new elements (`2^d` nodes each), already offset
    /// by the number of nodes the target mesh had.
    pub connectivity: Vec<Idx>,
    /// Mesh data `"tag_1"` (`Int`) of the new elements: running cell
    /// number from 0 in cell order.
    pub tags: Vec<Int>,
}

/// Dynamic spatial hash grid (upstream `SpatialGrid<T>`).
#[derive(Clone, Debug)]
pub struct SpatialGrid<T> {
    dimension: usize,
    spacing: Vec<Real>,
    center: Vec<Real>,
    lower: Vec<Real>,
    upper: Vec<Real>,
    cells: BTreeMap<CellId, Vec<T>>,
}

impl<T> SpatialGrid<T> {
    /// Empty grid of dimension `dimension` (upstream `SpatialGrid(dim)`):
    /// bounds `+Real::MAX` / `−Real::MAX`, spacing and center zero until
    /// set.
    #[must_use]
    pub fn new(dimension: usize) -> Self {
        Self {
            dimension,
            spacing: vec![0.0; dimension],
            center: vec![0.0; dimension],
            lower: vec![Real::MAX; dimension],
            upper: vec![-Real::MAX; dimension],
            cells: BTreeMap::new(),
        }
    }

    /// Empty grid with spacing and center (upstream
    /// `SpatialGrid(dim, spacing, center)`).
    ///
    /// # Errors
    /// [`Error::DimensionMismatch`] if a vector does not have `dimension`
    /// values.
    pub fn with_spacing(dimension: usize, spacing: &[Real], center: &[Real]) -> Result<Self> {
        let mut g = Self::new(dimension);
        g.set_spacing(spacing)?;
        g.set_center(center)?;
        Ok(g)
    }

    fn check_len(&self, context: &'static str, got: usize) -> Result<()> {
        if got == self.dimension {
            Ok(())
        } else {
            Err(Error::DimensionMismatch {
                context,
                expected: self.dimension,
                got,
            })
        }
    }

    /// Set the cell spacing (upstream `setSpacing`).
    ///
    /// # Errors
    /// [`Error::DimensionMismatch`] on a wrong length.
    pub fn set_spacing(&mut self, spacing: &[Real]) -> Result<()> {
        self.check_len("SpatialGrid::set_spacing", spacing.len())?;
        self.spacing.copy_from_slice(spacing);
        Ok(())
    }

    /// Set the grid origin (upstream `setCenter`).
    ///
    /// # Errors
    /// [`Error::DimensionMismatch`] on a wrong length.
    pub fn set_center(&mut self, center: &[Real]) -> Result<()> {
        self.check_len("SpatialGrid::set_center", center.len())?;
        self.center.copy_from_slice(center);
        Ok(())
    }

    /// Dimension.
    #[must_use]
    pub fn dimension(&self) -> usize {
        self.dimension
    }

    /// Cell spacing (upstream `getSpacing`).
    #[must_use]
    pub fn spacing(&self) -> &[Real] {
        &self.spacing
    }

    /// Grid origin (upstream `getCenter`).
    #[must_use]
    pub fn center(&self) -> &[Real] {
        &self.center
    }

    /// Lower corner of the union of the non-empty cells (upstream
    /// `getLowerBounds`).
    #[must_use]
    pub fn lower_bounds(&self) -> &[Real] {
        &self.lower
    }

    /// Upper corner of the union of the non-empty cells (upstream
    /// `getUpperBounds`).
    #[must_use]
    pub fn upper_bounds(&self) -> &[Real] {
        &self.upper
    }

    /// Cell id of the coordinate `x` in direction `direction` (upstream
    /// `getCellID(Real position, Int direction)`, private upstream but
    /// called by its `friend` `GridSynchronizer` on the bounds of the box
    /// intersections, `grid_synchronizer.cc:88-89`; D-048 makes it public):
    /// `floor((x − center[direction]) / spacing[direction])`, one rounded
    /// subtraction, one rounded division, then `floor`, exactly as
    /// upstream.
    ///
    /// The id is checked instead of converted blindly: upstream's
    /// `Int id = std::floor(…)` is undefined for a NaN or a value outside
    /// `Int` (x86-64 yields `INT_MIN`, aarch64 saturates and maps NaN to 0;
    /// UD-270); here a non-finite quotient or one outside the admissible
    /// range `(Idx::MIN, Idx::MAX)` (see [`CellId::new`]) is an error, so
    /// every id the grid hands out has neighbours that do not overflow.
    ///
    /// # Errors
    /// [`Error::OutOfRange`] if `direction ≥ dimension()` (upstream:
    /// `AKANTU_DEBUG_ASSERT`, active in the oracle build);
    /// [`Error::Numerical`] for a non-finite or inadmissible id (e.g. a NaN
    /// coordinate, a zero spacing, or a coordinate more than about
    /// `2^31 · spacing` away from the center).
    pub fn cell_index(&self, x: Real, direction: usize) -> Result<Idx> {
        let (Some(&c), Some(&h)) = (self.center.get(direction), self.spacing.get(direction)) else {
            return Err(Error::OutOfRange {
                what: "spatial grid direction",
                index: direction,
                len: self.dimension,
            });
        };
        let dist_center = x - c;
        let id = (dist_center / h).floor();
        // admissible: strictly inside (Idx::MIN, Idx::MAX), see CellId::new
        if id.is_finite() && id > Real::from(Idx::MIN) && id < Real::from(Idx::MAX) {
            #[allow(clippy::cast_possible_truncation)] // range checked above, integral value
            Ok(id as Idx)
        } else {
            Err(Error::Numerical(format!(
                "spatial grid: coordinate {x} in direction {direction} gives the cell id {id}, \
                 outside the representable range"
            )))
        }
    }

    /// Cell containing `position` (upstream `getCellID(position)`).
    ///
    /// # Errors
    /// [`Error::DimensionMismatch`] if `position` does not have `dimension`
    /// values; [`Error::Numerical`] for a non-finite or unrepresentable id.
    pub fn cell_id(&self, position: &[Real]) -> Result<CellId> {
        self.check_len("SpatialGrid::cell_id", position.len())?;
        let ids = position
            .iter()
            .enumerate()
            .map(|(d, &x)| self.cell_index(x, d))
            .collect::<Result<Vec<_>>>()?;
        Ok(CellId { ids })
    }

    /// Insert `item` in the cell containing `position` (upstream `insert`);
    /// a new cell extends the bounds by its corners
    /// `center + id·spacing` and `+ spacing`. Returns the cell's items.
    ///
    /// # Errors
    /// As [`SpatialGrid::cell_id`]; nothing is inserted on error.
    pub fn insert(&mut self, item: T, position: &[Real]) -> Result<&[T]> {
        let id = self.cell_id(position)?;
        if !self.cells.contains_key(&id) {
            for d in 0..self.dimension {
                let posl = self.center[d] + Real::from(id.ids[d]) * self.spacing[d];
                let posu = posl + self.spacing[d];
                // Eigen cwise min/max = std::min/std::max
                if posl < self.lower[d] {
                    self.lower[d] = posl;
                }
                if self.upper[d] < posu {
                    self.upper[d] = posu;
                }
            }
        }
        let cell = self.cells.entry(id).or_default();
        cell.push(item);
        Ok(cell)
    }

    /// Items of a cell, empty if the cell does not exist (upstream
    /// `getCell`, `beginCell`/`endCell`).
    #[must_use]
    pub fn cell(&self, id: &CellId) -> &[T] {
        self.cells.get(id).map_or(&[], Vec::as_slice)
    }

    /// Items of the cell containing `position` (upstream
    /// `getCell(position)`).
    ///
    /// # Errors
    /// As [`SpatialGrid::cell_id`].
    pub fn cell_at(&self, position: &[Real]) -> Result<&[T]> {
        Ok(self.cell(&self.cell_id(position)?))
    }

    /// Non-empty cells with their items, in cell order (upstream
    /// `begin()`/`end()` over the cell map).
    pub fn cells(&self) -> impl ExactSizeIterator<Item = (&CellId, &[T])> + '_ {
        self.cells.iter().map(|(k, v)| (k, v.as_slice()))
    }

    /// Ids of the non-empty cells, in cell order.
    pub fn cell_ids(&self) -> impl ExactSizeIterator<Item = &CellId> + '_ {
        self.cells.keys()
    }

    /// Non-empty cells whose ids lie in the closed box `[first, last]`
    /// (every direction `d`: `first[d] ≤ id[d] ≤ last[d]`), with their
    /// items, in cell order.
    ///
    /// This is what upstream's `GridSynchronizer` visits
    /// (`grid_synchronizer.cc:98-125`): nested loops over the range, first
    /// direction outermost, `getCell` of each id. That enumeration is the
    /// lexicographic order of the ids, i.e. the cell order, and an absent
    /// cell contributes nothing, so the visited (cell, item) sequence is
    /// identical; only the non-empty cells of the range are looked at
    /// (a [`BTreeMap`] range scan) instead of every id of the box. An
    /// inverted range (`first[d] > last[d]` in some direction) visits
    /// nothing, as upstream's loops.
    ///
    /// # Errors
    /// [`Error::DimensionMismatch`] if `first` or `last` does not have
    /// `dimension()` ids.
    pub fn cells_in_range<'a>(
        &'a self,
        first: &'a CellId,
        last: &'a CellId,
    ) -> Result<impl Iterator<Item = (&'a CellId, &'a [T])> + 'a> {
        self.check_len("SpatialGrid::cells_in_range (first)", first.dim())?;
        self.check_len("SpatialGrid::cells_in_range (last)", last.dim())?;
        // all first[d] ≤ last[d] ⇒ first ≤ last lexicographically, so the
        // map range is valid (BTreeMap::range panics on start > end)
        let non_empty = first.ids.iter().zip(&last.ids).all(|(f, l)| f <= l);
        let range = non_empty.then(|| self.cells.range(first..=last));
        Ok(range.into_iter().flatten().filter_map(move |(id, items)| {
            let inside = id
                .ids
                .iter()
                .zip(first.ids.iter().zip(&last.ids))
                .all(|(c, (f, l))| f <= c && c <= l);
            inside.then_some((id, items.as_slice()))
        }))
    }

    /// Number of non-empty cells.
    #[must_use]
    pub fn nb_cells(&self) -> usize {
        self.cells.len()
    }

    /// Items of the `3^d` cells around the cell containing `position`
    /// (that cell included), one slice per neighbouring cell (empty for an
    /// absent cell), in upstream's order: what
    /// `for (auto && n : grid.getCellID(position).neighbors())
    /// for (auto && item : grid.getCell(n))` visits (the contact detector's
    /// local search, `contact_detector.cc:165-170`). The cells come in the
    /// order of [`CellId::neighbors`] (first coordinate fastest, the
    /// edge-cell rule included) and the items of a cell
    /// in insertion order, so the sequence equals
    /// `self.cell_id(position)?.neighbors().map(|n| self.cell(&n))`.
    ///
    /// Unlike that expression the walk allocates nothing once `scratch`
    /// has held a cell id of this dimension: the cell ids are built in
    /// place in `scratch` (searches that walk a neighbourhood per node,
    /// AGENTS, "Rules for numerical code").
    ///
    /// # Errors
    /// As [`SpatialGrid::cell_id`].
    pub fn neighborhood_at<'a, 's>(
        &'a self,
        position: &[Real],
        scratch: &'s mut NeighborhoodScratch,
    ) -> Result<NeighborhoodItems<'a, 's, T>> {
        self.check_len("SpatialGrid::neighborhood_at", position.len())?;
        scratch.center.clear();
        for (d, &x) in position.iter().enumerate() {
            let id = self.cell_index(x, d)?;
            scratch.center.push(id);
        }
        scratch.offset.clear();
        scratch.offset.resize(self.dimension, -1);
        scratch.cell.ids.clear();
        scratch.cell.ids.resize(self.dimension, 0);
        Ok(NeighborhoodItems {
            grid: self,
            scratch,
            done: false,
        })
    }

    /// Text of upstream `printself(stream, indent)` with `T`'s demangled
    /// C++ name `type_name` (e.g. `"akantu::Element"`): values with
    /// precision 5 (`%.5g`), `nb_cells` followed by the number of cells of
    /// the bounding box `Π ceil((upper − lower) / spacing)`.
    #[must_use]
    pub fn print_self(&self, type_name: &str, indent: usize) -> String {
        let space = " ".repeat(indent);
        let list = |v: &[Real]| {
            v.iter()
                .map(|&x| format_g(x, 5))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut s = String::new();
        let _ = writeln!(s, "{space}SpatialGrid<{type_name}> [");
        let _ = writeln!(s, "{space} + dimension    : {}", self.dimension);
        let _ = writeln!(s, "{space} + lower bounds : {{{}}}", list(&self.lower));
        let _ = writeln!(s, "{space} + upper bounds : {{{}}}", list(&self.upper));
        let _ = writeln!(s, "{space} + spacing : {{{}}}", list(&self.spacing));
        let _ = writeln!(s, "{space} + center : {{{}}}", list(&self.center));
        let dist: Vec<Real> = (0..self.dimension)
            .map(|i| (self.upper[i] - self.lower[i]) / self.spacing[i])
            .collect();
        // upstream reads dist(0) unconditionally (dimension ≥ 1)
        let mut nb = dist.first().copied().unwrap_or(0.0).ceil();
        for d in dist.iter().skip(1) {
            nb *= d.ceil();
        }
        let _ = writeln!(
            s,
            "{space} + nb_cells     : {}/{}",
            self.cells.len(),
            format_g(nb, 5)
        );
        let _ = writeln!(s, "{space}]");
        s
    }

    /// Nodes, connectivity and `"tag_1"` data of upstream `saveAsMesh` for
    /// a target mesh that already has `node_offset` nodes: per non-empty
    /// cell (cell order) `2^d` new nodes computed with upstream's sequence
    /// of `±spacing` updates (so the bits match), one element and its tag.
    ///
    /// # Errors
    /// [`Error::Unsupported`] for a dimension other than 1, 2 or 3;
    /// [`Error::CapacityExceeded`] if the node numbers exceed [`Idx`].
    pub fn mesh_arrays(&self, node_offset: usize) -> Result<GridMeshArrays> {
        let dim = self.dimension;
        let element_type = match dim {
            1 => ElementType::Segment2,
            2 => ElementType::Quadrangle4,
            3 => ElementType::Hexahedron8,
            _ => {
                return Err(Error::Unsupported(format!(
                    "saving a {dim}-dimensional spatial grid as a mesh"
                )));
            }
        };
        let per_cell = 1usize << dim;
        let nb_new = self.cells.len().saturating_mul(per_cell);
        let total = node_offset.saturating_add(nb_new);
        if total > crate::types::MAX_LOCAL_COUNT {
            return Err(Error::CapacityExceeded {
                what: "spatial grid mesh nodes",
                requested: total as u128,
                limit: crate::types::MAX_LOCAL_COUNT as u128,
            });
        }
        let mut nodes = Vec::with_capacity(nb_new * dim);
        let mut connectivity = Vec::with_capacity(nb_new);
        let mut tags = Vec::with_capacity(self.cells.len());
        let s = &self.spacing;
        let mut pos = vec![0.0; dim];
        for (global_id, id) in self.cells.keys().enumerate() {
            let cur = crate::types::to_idx(node_offset + global_id * per_cell);
            for (i, p) in pos.iter_mut().enumerate() {
                *p = self.center[i] + Real::from(id.ids[i]) * s[i];
            }
            nodes.extend_from_slice(&pos);
            for (p, si) in pos.iter_mut().zip(s) {
                *p += si;
            }
            nodes.extend_from_slice(&pos);
            match dim {
                1 => connectivity.extend_from_slice(&[cur, cur + 1]),
                2 => {
                    pos[0] -= s[0];
                    nodes.extend_from_slice(&pos);
                    pos[0] += s[0];
                    pos[1] -= s[1];
                    nodes.extend_from_slice(&pos);
                    connectivity.extend_from_slice(&[cur, cur + 3, cur + 1, cur + 2]);
                }
                _ => {
                    pos[1] -= s[1];
                    pos[2] -= s[2];
                    nodes.extend_from_slice(&pos);
                    pos[1] += s[1];
                    nodes.extend_from_slice(&pos);
                    pos[0] -= s[0];
                    nodes.extend_from_slice(&pos);

                    pos[1] -= s[1];
                    pos[2] += s[2];
                    nodes.extend_from_slice(&pos);
                    pos[0] += s[0];
                    nodes.extend_from_slice(&pos);
                    pos[0] -= s[0];
                    pos[1] += s[1];
                    nodes.extend_from_slice(&pos);
                    connectivity.extend_from_slice(&[
                        cur,
                        cur + 2,
                        cur + 3,
                        cur + 4,
                        cur + 5,
                        cur + 6,
                        cur + 1,
                        cur + 7,
                    ]);
                }
            }
            tags.push(
                Int::try_from(global_id).map_err(|_| Error::CapacityExceeded {
                    what: "spatial grid cell tag",
                    requested: global_id as u128,
                    limit: Int::MAX as u128,
                })?,
            );
        }
        Ok(GridMeshArrays {
            element_type,
            nodes,
            connectivity,
            tags,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighbors_first_coordinate_fastest() {
        let c = CellId::new(vec![5, -2]).unwrap();
        let n: Vec<Vec<Idx>> = c.neighbors().map(|x| x.ids().to_vec()).collect();
        assert_eq!(
            n,
            vec![
                vec![4, -3],
                vec![5, -3],
                vec![6, -3],
                vec![4, -2],
                vec![5, -2],
                vec![6, -2],
                vec![4, -1],
                vec![5, -1],
                vec![6, -1],
            ]
        );
        assert_eq!(CellId::new(vec![0, 0, 0]).unwrap().neighbors().count(), 27);
        assert_eq!(CellId::new(vec![7]).unwrap().neighbors().count(), 3);
        assert!(CellId::new(vec![Idx::MAX]).is_err());
    }

    /// D-048: the allocation-free walk the contact
    /// detector uses visits the same (cell, item) sequence as upstream's
    /// `getCellID(position).neighbors()` + `getCell`, i.e. as the
    /// allocating `cell_id` + `CellId::neighbors` + `cell`, in 1-, 2- and
    /// 3-D, with the scratch reused across positions and dimensions, and
    /// at edge cells.
    #[test]
    fn neighborhood_walk_equals_neighbors_then_cell() {
        let mut scratch = NeighborhoodScratch::default();
        let mut seed = 0x2545_f491_u32;
        let mut next = move || {
            // xorshift32: deterministic pseudo-random coordinates
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            Real::from(seed % 2001) / 1000.0 - 1.0
        };
        for dim in [3usize, 1, 2, 3] {
            let spacing: Vec<Real> = (0..dim)
                .map(|d| 0.17 + 0.05 * Real::from(d as u8))
                .collect();
            let center: Vec<Real> = (0..dim).map(|d| 0.01 * Real::from(d as u8)).collect();
            let mut g = SpatialGrid::with_spacing(dim, &spacing, &center).unwrap();
            for item in 0..200u32 {
                let p: Vec<Real> = (0..dim).map(|_| next()).collect();
                g.insert(item, &p).unwrap();
            }
            for _ in 0..100 {
                let p: Vec<Real> = (0..dim).map(|_| next()).collect();
                let expected: Vec<Vec<u32>> = g
                    .cell_id(&p)
                    .unwrap()
                    .neighbors()
                    .map(|n| g.cell(&n).to_vec())
                    .collect();
                let walked: Vec<Vec<u32>> = g
                    .neighborhood_at(&p, &mut scratch)
                    .unwrap()
                    .map(<[u32]>::to_vec)
                    .collect();
                assert_eq!(walked.len(), 3usize.pow(u32::try_from(dim).unwrap()));
                assert_eq!(walked, expected, "dim {dim}, position {p:?}");
            }
            // the second walk of the same dimension reuses the buffers
            let p = vec![0.0; dim];
            let _ = g.neighborhood_at(&p, &mut scratch).unwrap().count();
            let ptr = scratch.cell.ids.as_ptr();
            let _ = g.neighborhood_at(&p, &mut scratch).unwrap().count();
            assert_eq!(scratch.cell.ids.as_ptr(), ptr);
            // errors as cell_id
            assert!(
                g.neighborhood_at(&vec![0.0; dim + 1], &mut scratch)
                    .is_err()
            );
            assert!(
                g.neighborhood_at(&vec![Real::NAN; dim], &mut scratch)
                    .is_err()
            );
        }
        // edge cell: two neighbours, the items of the occupied one
        let mut g = SpatialGrid::with_spacing(1, &[1.0], &[0.0]).unwrap();
        let x = Real::from(Idx::MIN) + 1.5;
        g.insert(7u8, &[x]).unwrap();
        let walked: Vec<Vec<u8>> = g
            .neighborhood_at(&[x], &mut scratch)
            .unwrap()
            .map(<[u8]>::to_vec)
            .collect();
        assert_eq!(walked, vec![vec![7], vec![]]);
    }

    /// The neighbours of an edge cell (an id next to
    /// `Idx::MIN` or `Idx::MAX`) used to be built without the admissibility
    /// check, and the neighbours of such a neighbour overflowed (panic in
    /// debug builds, silent wrap in release). Every yielded neighbour must
    /// be admissible, so that recursing never overflows.
    #[test]
    fn neighbors_of_edge_cells_stay_admissible() {
        let admissible = |c: &CellId| c.ids().iter().all(|&i| i != Idx::MIN && i != Idx::MAX);
        for edge in [
            vec![Idx::MIN + 1],
            vec![Idx::MAX - 1],
            vec![Idx::MIN + 1, Idx::MAX - 1],
            vec![0, Idx::MIN + 1, Idx::MAX - 1],
        ] {
            let c = CellId::new(edge).unwrap();
            for n in c.neighbors() {
                assert!(admissible(&n), "{n:?}");
                for nn in n.neighbors() {
                    assert!(admissible(&nn), "{nn:?}");
                }
            }
        }
        // the first neighbour of MIN + 1, then its neighbours (this walk
        // left the admissible range before the edge-cell rule)
        let c = CellId::new(vec![Idx::MIN + 1]).unwrap();
        let first = c.neighbors().next().unwrap();
        assert_eq!(first.ids(), &[Idx::MIN + 1]);
        assert_eq!(first.neighbors().count(), 2);
        // the same through a valid position: spacing 1, centre 0,
        // x = MIN + 1.5 lies in the cell MIN + 1
        let mut g = SpatialGrid::with_spacing(1, &[1.0], &[0.0]).unwrap();
        let x = Real::from(Idx::MIN) + 1.5;
        g.insert(7u8, &[x]).unwrap();
        let id = g.cell_id(&[x]).unwrap();
        assert_eq!(id.ids(), &[Idx::MIN + 1]);
        let around: Vec<Vec<Idx>> = id
            .neighbors()
            .flat_map(|n| n.neighbors().map(|m| m.ids().to_vec()).collect::<Vec<_>>())
            .collect();
        assert_eq!(
            around,
            vec![
                vec![Idx::MIN + 1],
                vec![Idx::MIN + 2],
                vec![Idx::MIN + 1],
                vec![Idx::MIN + 2],
                vec![Idx::MIN + 3],
            ]
        );
        // skipped cells can hold nothing: no item is lost
        let items: Vec<u8> = id.neighbors().flat_map(|n| g.cell(&n).to_vec()).collect();
        assert_eq!(items, vec![7]);
        // the interior is unchanged: 3^d neighbours
        assert_eq!(
            CellId::new(vec![Idx::MIN + 2]).unwrap().neighbors().count(),
            3
        );
        assert_eq!(
            CellId::new(vec![Idx::MAX - 1, 0])
                .unwrap()
                .neighbors()
                .count(),
            6
        );
    }

    /// What upstream's `GridSynchronizer` uses as
    /// a `friend` (`grid_synchronizer.cc:52-120`): the per-direction cell id
    /// of the intersection bounds (`getCellID(Real, Int)`), then every cell
    /// of the closed id range (`CellID(dim)` + `setID`, `getCell`).
    #[test]
    fn grid_synchronizer_access_is_public() {
        let mut g = SpatialGrid::with_spacing(2, &[0.25, 0.5], &[0.1, -0.2]).unwrap();
        let pts = [
            [0.0, 0.0],
            [0.3, 0.9],
            [-0.6, -0.4],
            [0.12, 0.31],
            [1.4, -1.1],
            [0.35, 0.05],
            [-0.15, 0.6],
        ];
        for (i, p) in pts.iter().enumerate() {
            g.insert(i, p).unwrap();
        }
        // per-direction id = the components of cell_id
        for p in &pts {
            let id = g.cell_id(p).unwrap();
            for (d, &x) in p.iter().enumerate() {
                assert_eq!(g.cell_index(x, d).unwrap(), id.ids()[d]);
            }
        }
        assert_eq!(g.cell_index(0.1, 0).unwrap(), 0);
        assert_eq!(g.cell_index(0.0999, 0).unwrap(), -1);
        assert!(g.cell_index(0.0, 2).is_err()); // upstream: debug assertion
        assert!(g.cell_index(f64::NAN, 0).is_err());
        assert!(g.cell_index(f64::INFINITY, 1).is_err());
        assert!(g.cell_index(1e300, 0).is_err());
        // an intersection box, its first/last cells, the elements of the
        // cells of the range: upstream's enumeration (first direction
        // outermost) over getCell, against cells_in_range
        let (lo, up) = ([-0.3, -0.45], [0.4, 0.7]);
        let first = CellId::new(vec![
            g.cell_index(lo[0], 0).unwrap(),
            g.cell_index(lo[1], 1).unwrap(),
        ])
        .unwrap();
        let mut last = CellId::new(vec![0, 0]).unwrap();
        for (d, &x) in up.iter().enumerate() {
            last.set_id(d, g.cell_index(x, d).unwrap()).unwrap();
        }
        let mut upstream_order = Vec::new();
        for fd in first.ids()[0]..=last.ids()[0] {
            for sd in first.ids()[1]..=last.ids()[1] {
                let id = CellId::new(vec![fd, sd]).unwrap();
                for &item in g.cell(&id) {
                    upstream_order.push((id.clone(), item));
                }
            }
        }
        let mine: Vec<(CellId, usize)> = g
            .cells_in_range(&first, &last)
            .unwrap()
            .flat_map(|(id, items)| items.iter().map(move |&i| (id.clone(), i)))
            .collect();
        assert_eq!(mine, upstream_order);
        assert!(!mine.is_empty() && mine.len() < pts.len());
        // an inverted range visits nothing (upstream's loops do not run)
        assert_eq!(g.cells_in_range(&last, &first).unwrap().count(), 0);
        // wrong dimension / inadmissible id
        let one = CellId::new(vec![0]).unwrap();
        assert!(g.cells_in_range(&one, &last).is_err());
        assert!(last.set_id(2, 0).is_err());
        assert!(last.set_id(0, Idx::MAX).is_err());
    }

    #[test]
    fn cell_ids_floor_and_lexicographic_order() {
        let mut g = SpatialGrid::with_spacing(2, &[0.5, 1.0], &[0.0, 0.0]).unwrap();
        assert_eq!(g.cell_id(&[-0.1, 0.0]).unwrap().ids(), &[-1, 0]);
        assert_eq!(g.cell_id(&[0.5, -1.0]).unwrap().ids(), &[1, -1]);
        g.insert('a', &[0.6, 0.2]).unwrap();
        g.insert('b', &[-0.1, 3.0]).unwrap();
        g.insert('c', &[0.7, 0.9]).unwrap();
        let order: Vec<(Vec<Idx>, Vec<char>)> = g
            .cells()
            .map(|(k, v)| (k.ids().to_vec(), v.to_vec()))
            .collect();
        assert_eq!(
            order,
            vec![(vec![-1, 3], vec!['b']), (vec![1, 0], vec!['a', 'c'])]
        );
        assert_eq!(g.lower_bounds(), &[-0.5, 0.0]);
        assert_eq!(g.upper_bounds(), &[1.0, 4.0]);
        assert_eq!(g.cell_at(&[0.75, 0.5]).unwrap(), &['a', 'c']);
        assert!(g.cell(&CellId::new(vec![9, 9]).unwrap()).is_empty());
        assert!(g.insert('x', &[f64::NAN, 0.0]).is_err());
        assert!(g.insert('x', &[1.0]).is_err());
        assert_eq!(g.nb_cells(), 2);
    }

    #[test]
    fn print_self_layout() {
        let mut g = SpatialGrid::with_spacing(2, &[0.2, 0.2], &[0.0, 0.0]).unwrap();
        g.insert(0, &[0.1, 0.1]).unwrap();
        g.insert(1, &[-0.3, 0.1]).unwrap();
        // x extent: (0.2 − (−0.4)) / 0.2 = 3.0000000000000004 → ceil 4 (the
        // floating-point bounding-box count, as upstream)
        assert_eq!(
            g.print_self("int", 0),
            "SpatialGrid<int> [\n + dimension    : 2\n + lower bounds : {-0.4, 0}\n \
             + upper bounds : {0.2, 0.2}\n + spacing : {0.2, 0.2}\n + center : {0, 0}\n \
             + nb_cells     : 2/4\n]\n"
        );
    }

    #[test]
    fn mesh_arrays_follow_upstream_node_sequence() {
        let mut g = SpatialGrid::with_spacing(2, &[0.1, 0.3], &[0.0, 0.0]).unwrap();
        g.insert(0u8, &[0.25, 0.05]).unwrap();
        let m = g.mesh_arrays(4).unwrap();
        assert_eq!(m.element_type, ElementType::Quadrangle4);
        assert_eq!(m.connectivity, vec![4, 7, 5, 6]);
        assert_eq!(m.tags, vec![0]);
        let (x0, y0) = (0.0 + 2.0 * 0.1, 0.0 + 0.0 * 0.3);
        let (x1, y1) = (x0 + 0.1, y0 + 0.3);
        assert_eq!(
            m.nodes,
            vec![x0, y0, x1, y1, x1 - 0.1, y1, (x1 - 0.1) + 0.1, y1 - 0.3]
        );
        let mut g3 = SpatialGrid::with_spacing(3, &[1.0; 3], &[0.0; 3]).unwrap();
        g3.insert((), &[0.5, 0.5, 0.5]).unwrap();
        let m3 = g3.mesh_arrays(0).unwrap();
        assert_eq!(m3.connectivity, vec![0, 2, 3, 4, 5, 6, 1, 7]);
        let corners: Vec<&[Real]> = m3
            .connectivity
            .iter()
            .map(|&n| &m3.nodes[3 * n as usize..3 * n as usize + 3])
            .collect();
        assert_eq!(
            corners,
            vec![
                &[0.0, 0.0, 0.0][..],
                &[1.0, 0.0, 0.0],
                &[1.0, 1.0, 0.0],
                &[0.0, 1.0, 0.0],
                &[0.0, 0.0, 1.0],
                &[1.0, 0.0, 1.0],
                &[1.0, 1.0, 1.0],
                &[0.0, 1.0, 1.0],
            ]
        );
        assert!(SpatialGrid::<u8>::new(4).mesh_arrays(0).is_err());
    }
}
