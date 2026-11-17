// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs extension (no Akantu or Eigen counterpart): a nested-dissection
// column ordering for the Eigen SparseLU translation (D-125).

//! Nested-dissection fill-reducing ordering of a sparse matrix's columns,
//! deterministic and written for this crate (no partitioning library).
//!
//! The graph is that of `A + Aᵀ` without the diagonal. Vertices with the
//! same closed neighbourhood (the unknowns of one node of a finite-element
//! mesh) are merged into weighted supervariables first. The compressed
//! graph is then split recursively:
//!
//! 1. **Bisection**, multilevel: heavy-edge matching down to about
//!    [`COARSEN_TO`] vertices; on the coarsest graph a breadth-first
//!    growth from [`INIT_TRIES`] seeds (a pseudo-peripheral vertex and
//!    evenly spaced ones), each refined; then, level by level back to the
//!    full graph, a Fiduccia–Mattheyses refinement of the edge cut under
//!    a balance bound ([`IMBALANCE`]).
//! 2. **Vertex separator**: the minimum-weight vertex cover of the cut
//!    edges (a bipartite graph between the two boundaries; König's
//!    theorem, computed as a minimum cut with Dinic's maximum flow).
//! 3. The two parts are ordered first (recursively, each connected
//!    component separately), the separator last. A part of at most
//!    [`LEAF`] supervariables is ordered by minimum degree on its own
//!    graph.
//!
//! Every choice is a function of the pattern only: vertices are visited in
//! index order or in an order sorted with the index as the last key, ties
//! go to the smaller index, and no hash-map iteration or random number is
//! involved. The same pattern gives the same permutation on every machine
//! and for every thread count (the ordering runs on one thread).

use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

/// Largest number of supervariables ordered by minimum degree without a
/// further dissection.
pub(crate) const LEAF: usize = 64;
/// Coarsening stops at this many vertices.
pub(crate) const COARSEN_TO: usize = 100;
/// Seeds of the initial bisection on the coarsest graph.
pub(crate) const INIT_TRIES: usize = 4;
/// Allowed deviation of a part's weight from half the total.
pub(crate) const IMBALANCE: f64 = 0.05;
/// Moves without improvement after which a refinement pass stops (50:
/// the fill of 100 on the 3-D grids of `examples/nested_dissection_fill.rs`,
/// 27 % less ordering time).
const FM_MAX_STALL: usize = 50;
/// Refinement passes per level (more did not change the fill).
const FM_PASSES: usize = 4;
/// Largest subgraph ordered by minimum degree when a bisection fails (a
/// nearly complete graph); larger ones keep the index order.
const MD_FALLBACK_MAX: usize = 1024;
const NONE: usize = usize::MAX;

/// An undirected graph in compressed sparse row form with vertex and edge
/// weights.
#[derive(Clone, Debug, Default)]
struct Graph {
    xadj: Vec<usize>,
    adj: Vec<usize>,
    ewgt: Vec<u64>,
    vwgt: Vec<u64>,
}

impl Graph {
    fn n(&self) -> usize {
        self.vwgt.len()
    }

    fn nbrs(&self, v: usize) -> std::ops::Range<usize> {
        self.xadj[v]..self.xadj[v + 1]
    }

    fn total_weight(&self) -> u64 {
        self.vwgt.iter().sum()
    }
}

/// The adjacency of `A + Aᵀ` without the diagonal: sorted, duplicate-free
/// neighbour lists (`xadj`, `adj`).
pub(crate) fn symmetric_pattern(
    n: usize,
    col_ptr: &[usize],
    row_idx: &[usize],
) -> (Vec<usize>, Vec<usize>) {
    let mut deg = vec![0usize; n];
    for j in 0..n {
        for &i in &row_idx[col_ptr[j]..col_ptr[j + 1]] {
            if i != j && i < n {
                deg[i] += 1;
                deg[j] += 1;
            }
        }
    }
    let mut xadj = vec![0usize; n + 1];
    for v in 0..n {
        xadj[v + 1] = xadj[v] + deg[v];
    }
    let mut fill = xadj[..n].to_vec();
    let mut adj = vec![0usize; xadj[n]];
    for j in 0..n {
        for &i in &row_idx[col_ptr[j]..col_ptr[j + 1]] {
            if i != j && i < n {
                adj[fill[i]] = j;
                fill[i] += 1;
                adj[fill[j]] = i;
                fill[j] += 1;
            }
        }
    }
    // sort and remove duplicates in place, then compact
    let mut out_x = vec![0usize; n + 1];
    let mut w = 0usize;
    for v in 0..n {
        let (a, b) = (xadj[v], xadj[v + 1]);
        adj[a..b].sort_unstable();
        let mut last = NONE;
        for k in a..b {
            let x = adj[k];
            if x != last {
                adj[w] = x;
                w += 1;
                last = x;
            }
        }
        out_x[v + 1] = w;
    }
    adj.truncate(w);
    (out_x, adj)
}

/// The closed neighbourhood `N[v] = adj(v) ∪ {v}` in increasing order
/// (`nbrs` sorted, without `v`).
fn closed(nbrs: &[usize], v: usize) -> impl Iterator<Item = usize> + '_ {
    let p = nbrs.partition_point(|&x| x < v);
    nbrs[..p]
        .iter()
        .copied()
        .chain(std::iter::once(v))
        .chain(nbrs[p..].iter().copied())
}

/// Whether the closed neighbourhoods of `u` and `v` are equal.
fn same_closed_nbhd(xadj: &[usize], adj: &[usize], u: usize, v: usize) -> bool {
    let a = &adj[xadj[u]..xadj[u + 1]];
    let b = &adj[xadj[v]..xadj[v + 1]];
    a.len() == b.len() && closed(a, u).eq(closed(b, v))
}

/// Supervariables: vertices with equal closed neighbourhoods. Returns the
/// compressed graph (vertex weight = group size, edge weight = the number
/// of original edges between two groups) and the members of each
/// compressed vertex (ascending). Compressed vertices are numbered in the
/// order of their smallest member.
fn compress(n: usize, xadj: &[usize], adj: &[usize]) -> (Graph, Vec<Vec<usize>>) {
    let key = |v: usize| -> (usize, u64) {
        let mut s = v as u64 + 1;
        for &x in &adj[xadj[v]..xadj[v + 1]] {
            s = s.wrapping_add(x as u64 + 1);
        }
        (xadj[v + 1] - xadj[v], s)
    };
    let keys: Vec<(usize, u64)> = (0..n).map(key).collect();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_unstable_by_key(|&v| (keys[v], v));
    let mut rep: Vec<usize> = (0..n).collect();
    let mut start = 0usize;
    while start < n {
        let mut end = start + 1;
        while end < n && keys[order[end]] == keys[order[start]] {
            end += 1;
        }
        for i in start + 1..end {
            let v = order[i];
            for &u in &order[start..i] {
                if rep[u] == u && same_closed_nbhd(xadj, adj, u, v) {
                    rep[v] = u;
                    break;
                }
            }
        }
        start = end;
    }
    let mut cid = vec![NONE; n];
    let mut members: Vec<Vec<usize>> = Vec::new();
    for v in 0..n {
        if rep[v] == v {
            cid[v] = members.len();
            members.push(Vec::new());
        }
    }
    for v in 0..n {
        members[cid[rep[v]]].push(v);
    }
    let nc = members.len();
    let vwgt: Vec<u64> = members.iter().map(|m| m.len() as u64).collect();
    let mut xc = vec![0usize; nc + 1];
    let mut ac = Vec::new();
    let mut ec = Vec::new();
    let mut seen = vec![NONE; nc];
    for c in 0..nc {
        let r = members[c][0];
        for &x in &adj[xadj[r]..xadj[r + 1]] {
            let d = cid[rep[x]];
            if d != c && seen[d] != c {
                seen[d] = c;
                ac.push(d);
                ec.push(vwgt[c] * vwgt[d]);
            }
        }
        xc[c + 1] = ac.len();
    }
    (
        Graph {
            xadj: xc,
            adj: ac,
            ewgt: ec,
            vwgt,
        },
        members,
    )
}

/// The subgraph induced by `verts` (local index = position in `verts`).
/// `loc` is a scratch map of the full graph's size, all `NONE` on entry
/// and on return.
fn subgraph(g: &Graph, verts: &[usize], loc: &mut [usize]) -> Graph {
    for (i, &v) in verts.iter().enumerate() {
        loc[v] = i;
    }
    let mut s = Graph {
        xadj: Vec::with_capacity(verts.len() + 1),
        adj: Vec::new(),
        ewgt: Vec::new(),
        vwgt: verts.iter().map(|&v| g.vwgt[v]).collect(),
    };
    s.xadj.push(0);
    for &v in verts {
        for k in g.nbrs(v) {
            let l = loc[g.adj[k]];
            if l != NONE {
                s.adj.push(l);
                s.ewgt.push(g.ewgt[k]);
            }
        }
        s.xadj.push(s.adj.len());
    }
    for &v in verts {
        loc[v] = NONE;
    }
    s
}

/// Connected components, each as an ascending list of local vertices, in
/// the order of their smallest vertex.
fn components(g: &Graph) -> Vec<Vec<usize>> {
    let n = g.n();
    let mut comp = vec![NONE; n];
    let mut out = Vec::new();
    let mut queue = VecDeque::new();
    for s in 0..n {
        if comp[s] != NONE {
            continue;
        }
        let id = out.len();
        let mut list = vec![s];
        comp[s] = id;
        queue.push_back(s);
        while let Some(v) = queue.pop_front() {
            for k in g.nbrs(v) {
                let u = g.adj[k];
                if comp[u] == NONE {
                    comp[u] = id;
                    list.push(u);
                    queue.push_back(u);
                }
            }
        }
        list.sort_unstable();
        out.push(list);
    }
    out
}

/// Breadth-first levels from `s`: returns the last vertex reached (the
/// smallest index among the farthest ones) and the depth.
fn bfs_far(g: &Graph, s: usize, dist: &mut [usize]) -> (usize, usize) {
    dist.fill(NONE);
    dist[s] = 0;
    let mut queue = VecDeque::from([s]);
    let (mut far, mut depth) = (s, 0usize);
    while let Some(v) = queue.pop_front() {
        let d = dist[v];
        if d > depth || (d == depth && v < far) {
            far = v;
            depth = d;
        }
        for k in g.nbrs(v) {
            let u = g.adj[k];
            if dist[u] == NONE {
                dist[u] = d + 1;
                queue.push_back(u);
            }
        }
    }
    (far, depth)
}

/// A pseudo-peripheral vertex of `s`'s component (repeated farthest-vertex
/// searches while the depth grows, at most eight).
fn pseudo_peripheral(g: &Graph, s: usize) -> usize {
    let mut dist = vec![NONE; g.n()];
    let (mut v, mut depth) = bfs_far(g, s, &mut dist);
    for _ in 0..8 {
        let (w, d) = bfs_far(g, v, &mut dist);
        if d <= depth {
            break;
        }
        v = w;
        depth = d;
    }
    v
}

/// Heavy-edge matching: `cmap` (fine → coarse) and the coarse size.
/// Vertices are visited by increasing degree (then index); each takes the
/// unmatched neighbour of largest edge weight (then smallest weight, then
/// smallest index) whose combined weight stays at most `maxv`.
fn heavy_edge_matching(g: &Graph, maxv: u64) -> (Vec<usize>, usize) {
    let n = g.n();
    let mut visit: Vec<usize> = (0..n).collect();
    visit.sort_unstable_by_key(|&v| (g.xadj[v + 1] - g.xadj[v], v));
    let mut cmap = vec![NONE; n];
    let mut cn = 0usize;
    for &v in &visit {
        if cmap[v] != NONE {
            continue;
        }
        let mut best: Option<(u64, Reverse<u64>, Reverse<usize>)> = None;
        let mut mate = NONE;
        for k in g.nbrs(v) {
            let u = g.adj[k];
            if u == v || cmap[u] != NONE || g.vwgt[v] + g.vwgt[u] > maxv {
                continue;
            }
            let key = (g.ewgt[k], Reverse(g.vwgt[u]), Reverse(u));
            if best.is_none_or(|b| key > b) {
                best = Some(key);
                mate = u;
            }
        }
        cmap[v] = cn;
        if mate != NONE {
            cmap[mate] = cn;
        }
        cn += 1;
    }
    (cmap, cn)
}

/// The coarse graph of a matching (summed vertex and edge weights,
/// internal edges dropped).
fn contract(g: &Graph, cmap: &[usize], cn: usize) -> Graph {
    let n = g.n();
    // members of each coarse vertex (one or two), in fine index order
    let mut first = vec![NONE; cn];
    let mut second = vec![NONE; cn];
    for (v, &c) in cmap.iter().enumerate().take(n) {
        if first[c] == NONE {
            first[c] = v;
        } else {
            second[c] = v;
        }
    }
    let mut cg = Graph {
        xadj: Vec::with_capacity(cn + 1),
        adj: Vec::new(),
        ewgt: Vec::new(),
        vwgt: vec![0; cn],
    };
    cg.xadj.push(0);
    let mut slot = vec![NONE; cn];
    for c in 0..cn {
        let start = cg.adj.len();
        for v in [first[c], second[c]] {
            if v == NONE {
                continue;
            }
            cg.vwgt[c] += g.vwgt[v];
            for k in g.nbrs(v) {
                let d = cmap[g.adj[k]];
                if d == c {
                    continue;
                }
                if slot[d] == NONE {
                    slot[d] = cg.adj.len();
                    cg.adj.push(d);
                    cg.ewgt.push(g.ewgt[k]);
                } else {
                    cg.ewgt[slot[d]] += g.ewgt[k];
                }
            }
        }
        for k in start..cg.adj.len() {
            slot[cg.adj[k]] = NONE;
        }
        cg.xadj.push(cg.adj.len());
    }
    cg
}

/// State of a two-way partition: sides, part weights, internal and
/// external edge weights per vertex.
struct Bisection {
    part: Vec<u8>,
    pw: [u64; 2],
    id: Vec<u64>,
    ed: Vec<u64>,
}

impl Bisection {
    fn new(g: &Graph, part: Vec<u8>) -> Self {
        let n = g.n();
        let mut b = Self {
            part,
            pw: [0, 0],
            id: vec![0; n],
            ed: vec![0; n],
        };
        for v in 0..n {
            b.pw[usize::from(b.part[v])] += g.vwgt[v];
            for k in g.nbrs(v) {
                if b.part[g.adj[k]] == b.part[v] {
                    b.id[v] += g.ewgt[k];
                } else {
                    b.ed[v] += g.ewgt[k];
                }
            }
        }
        b
    }

    fn cut(&self) -> u64 {
        self.ed.iter().sum::<u64>() / 2
    }

    fn gain(&self, v: usize) -> i64 {
        self.ed[v] as i64 - self.id[v] as i64
    }

    /// Moves `v` to the other side.
    fn flip(&mut self, g: &Graph, v: usize) {
        let from = usize::from(self.part[v]);
        let to = 1 - from;
        self.part[v] = to as u8;
        self.pw[from] -= g.vwgt[v];
        self.pw[to] += g.vwgt[v];
        std::mem::swap(&mut self.id[v], &mut self.ed[v]);
        for k in g.nbrs(v) {
            let u = g.adj[k];
            let w = g.ewgt[k];
            if usize::from(self.part[u]) == to {
                self.id[u] += w;
                self.ed[u] -= w;
            } else {
                self.id[u] -= w;
                self.ed[u] += w;
            }
        }
    }
}

/// The heavier part's weight bound of a graph of total weight `total`.
fn weight_limit(total: u64, maxv: u64) -> u64 {
    let half = total.div_ceil(2);
    let lim = (total as f64 * (0.5 + IMBALANCE)).ceil() as u64;
    lim.max(half + maxv).min(total)
}

/// Fiduccia–Mattheyses refinement of the edge cut: passes of single
/// moves of the best-gain boundary vertex (each moved once per pass) under
/// the weight bound; each pass keeps its best prefix (balanced first, then
/// the smaller cut, then the smaller weight difference).
fn fm_refine(g: &Graph, b: &mut Bisection) {
    let n = g.n();
    if n < 2 {
        return;
    }
    let total = g.total_weight();
    let maxv = g.vwgt.iter().copied().max().unwrap_or(0);
    let limit = weight_limit(total, maxv);
    let score = |b: &Bisection, cut: u64| -> (bool, u64, u64) {
        let bal = b.pw[0] <= limit && b.pw[1] <= limit;
        (!bal, cut, b.pw[0].abs_diff(b.pw[1]))
    };
    let mut locked = vec![false; n];
    for _ in 0..FM_PASSES {
        locked.fill(false);
        let mut heaps: [BinaryHeap<(i64, Reverse<usize>)>; 2] =
            [BinaryHeap::new(), BinaryHeap::new()];
        for v in 0..n {
            if b.ed[v] > 0 {
                heaps[usize::from(b.part[v])].push((b.gain(v), Reverse(v)));
            }
        }
        let mut cut = b.cut();
        let mut best = score(b, cut);
        let start_score = best;
        let mut moves: Vec<usize> = Vec::new();
        let mut best_len = 0usize;
        loop {
            // valid tops of both heaps
            let mut top = [None, None];
            for s in 0..2 {
                while let Some(&(gn, Reverse(v))) = heaps[s].peek() {
                    if locked[v] || usize::from(b.part[v]) != s || gn != b.gain(v) {
                        heaps[s].pop();
                        continue;
                    }
                    if b.pw[1 - s] + g.vwgt[v] > limit && b.pw[s] <= limit {
                        // infeasible now; may become feasible later: keep
                        // it, but do not take it this time
                        top[s] = None;
                    } else {
                        top[s] = Some((gn, v));
                    }
                    break;
                }
            }
            let side = if b.pw[0] > limit {
                if top[0].is_some() { Some(0) } else { None }
            } else if b.pw[1] > limit {
                if top[1].is_some() { Some(1) } else { None }
            } else {
                match (top[0], top[1]) {
                    (Some((g0, _)), Some((g1, _))) => {
                        if g0 > g1 || (g0 == g1 && b.pw[0] >= b.pw[1]) {
                            Some(0)
                        } else {
                            Some(1)
                        }
                    }
                    (Some(_), None) => Some(0),
                    (None, Some(_)) => Some(1),
                    (None, None) => None,
                }
            };
            let Some(s) = side else { break };
            let Some((gn, v)) = top[s] else { break };
            heaps[s].pop();
            b.flip(g, v);
            locked[v] = true;
            cut = (cut as i64 - gn) as u64;
            moves.push(v);
            for k in g.nbrs(v) {
                let u = g.adj[k];
                if !locked[u] && b.ed[u] > 0 {
                    heaps[usize::from(b.part[u])].push((b.gain(u), Reverse(u)));
                }
            }
            let sc = score(b, cut);
            if sc < best {
                best = sc;
                best_len = moves.len();
            }
            if moves.len() - best_len > FM_MAX_STALL {
                break;
            }
        }
        for &v in moves[best_len..].iter().rev() {
            b.flip(g, v);
        }
        if best_len == 0 || best >= start_score {
            break;
        }
    }
}

/// Initial bisection of the coarsest graph: breadth-first growth of side 1
/// to half the weight from each seed, refined; the smallest cut wins
/// (balanced ones first; ties: the earlier seed).
fn initial_bisection(g: &Graph) -> Bisection {
    let n = g.n();
    let total = g.total_weight();
    let mut seeds = vec![pseudo_peripheral(g, 0)];
    for k in 1..INIT_TRIES {
        let s = k * n / INIT_TRIES;
        if !seeds.contains(&s) {
            seeds.push(s);
        }
    }
    let maxv = g.vwgt.iter().copied().max().unwrap_or(0);
    let limit = weight_limit(total, maxv);
    let mut best: Option<((bool, u64, u64), Bisection)> = None;
    for &s in &seeds {
        let mut part = vec![0u8; n];
        let mut w1 = 0u64;
        let mut seen = vec![false; n];
        let mut queue = VecDeque::from([s]);
        seen[s] = true;
        'grow: while w1 * 2 < total {
            let Some(v) = queue.pop_front() else {
                // disconnected remainder: continue from the smallest unseen
                match (0..n).find(|&u| !seen[u]) {
                    Some(u) => {
                        seen[u] = true;
                        queue.push_back(u);
                        continue 'grow;
                    }
                    None => break,
                }
            };
            part[v] = 1;
            w1 += g.vwgt[v];
            for k in g.nbrs(v) {
                let u = g.adj[k];
                if !seen[u] {
                    seen[u] = true;
                    queue.push_back(u);
                }
            }
        }
        let mut b = Bisection::new(g, part);
        fm_refine(g, &mut b);
        let bal = b.pw[0] <= limit && b.pw[1] <= limit;
        let key = (!bal, b.cut(), b.pw[0].abs_diff(b.pw[1]));
        if best.as_ref().is_none_or(|(k, _)| key < *k) {
            best = Some((key, b));
        }
    }
    best.map(|(_, b)| b)
        .unwrap_or_else(|| Bisection::new(g, vec![0; n]))
}

/// Maximum flow (Dinic) on a small network; used for the minimum-weight
/// vertex cover of the cut edges.
struct Flow {
    head: Vec<usize>,
    to: Vec<usize>,
    cap: Vec<u64>,
    next: Vec<usize>,
}

impl Flow {
    fn new(n: usize) -> Self {
        Self {
            head: vec![NONE; n],
            to: Vec::new(),
            cap: Vec::new(),
            next: Vec::new(),
        }
    }

    fn edge(&mut self, a: usize, b: usize, c: u64) {
        for (x, y, cc) in [(a, b, c), (b, a, 0)] {
            self.to.push(y);
            self.cap.push(cc);
            self.next.push(self.head[x]);
            self.head[x] = self.to.len() - 1;
        }
    }

    /// Levels from `s` in the residual network.
    fn levels(&self, s: usize) -> Vec<usize> {
        let mut lv = vec![NONE; self.head.len()];
        lv[s] = 0;
        let mut queue = VecDeque::from([s]);
        while let Some(v) = queue.pop_front() {
            let mut e = self.head[v];
            while e != NONE {
                let u = self.to[e];
                if self.cap[e] > 0 && lv[u] == NONE {
                    lv[u] = lv[v] + 1;
                    queue.push_back(u);
                }
                e = self.next[e];
            }
        }
        lv
    }

    fn augment(&mut self, v: usize, t: usize, f: u64, lv: &[usize], it: &mut [usize]) -> u64 {
        if v == t {
            return f;
        }
        while it[v] != NONE {
            let e = it[v];
            let u = self.to[e];
            if self.cap[e] > 0 && lv[u] == lv[v] + 1 {
                let d = self.augment(u, t, f.min(self.cap[e]), lv, it);
                if d > 0 {
                    self.cap[e] -= d;
                    self.cap[e ^ 1] += d;
                    return d;
                }
            }
            it[v] = self.next[e];
        }
        0
    }

    /// Runs the maximum flow and returns the vertices reachable from `s`
    /// in the final residual network.
    fn min_cut_side(&mut self, s: usize, t: usize) -> Vec<bool> {
        loop {
            let lv = self.levels(s);
            if lv[t] == NONE {
                return lv.iter().map(|&l| l != NONE).collect();
            }
            let mut it = self.head.clone();
            while self.augment(s, t, u64::MAX, &lv, &mut it) > 0 {}
        }
    }
}

/// Side 2 marks the separator: the minimum-weight vertex cover of the cut
/// edges of `b`.
fn vertex_separator(g: &Graph, b: &Bisection) -> Vec<u8> {
    let n = g.n();
    let mut idx = vec![NONE; n];
    let mut bnd = Vec::new();
    for (v, &e) in b.ed.iter().enumerate() {
        if e > 0 {
            idx[v] = bnd.len() + 2;
            bnd.push(v);
        }
    }
    let (s, t) = (0usize, 1usize);
    let mut f = Flow::new(bnd.len() + 2);
    let inf = g.total_weight() + 1;
    for &v in &bnd {
        if b.part[v] == 0 {
            f.edge(s, idx[v], g.vwgt[v]);
            for k in g.nbrs(v) {
                let u = g.adj[k];
                if b.part[u] == 1 {
                    f.edge(idx[v], idx[u], inf);
                }
            }
        } else {
            f.edge(idx[v], t, g.vwgt[v]);
        }
    }
    let reach = f.min_cut_side(s, t);
    let mut side = b.part.clone();
    for &v in &bnd {
        let r = reach[idx[v]];
        if (b.part[v] == 0 && !r) || (b.part[v] == 1 && r) {
            side[v] = 2;
        }
    }
    side
}

/// Multilevel bisection with a vertex separator: side 0, 1 or 2 per
/// vertex. `None` when no proper split exists (a part would be empty).
fn bisect(g: &Graph) -> Option<Vec<u8>> {
    let total = g.total_weight();
    let maxv = (1.5 * total as f64 / COARSEN_TO as f64).ceil().max(1.0) as u64;
    let mut graphs: Vec<Graph> = vec![g.clone()];
    let mut cmaps: Vec<Vec<usize>> = Vec::new();
    while let Some(last) = graphs.last()
        && last.n() > COARSEN_TO
    {
        let (cmap, cn) = heavy_edge_matching(last, maxv);
        if cn as f64 > 0.95 * last.n() as f64 {
            break;
        }
        let cg = contract(last, &cmap, cn);
        cmaps.push(cmap);
        graphs.push(cg);
    }
    let mut b = initial_bisection(graphs.last()?);
    for lvl in (0..cmaps.len()).rev() {
        let fine = &graphs[lvl];
        let part: Vec<u8> = cmaps[lvl].iter().map(|&c| b.part[c]).collect();
        b = Bisection::new(fine, part);
        fm_refine(fine, &mut b);
    }
    let side = vertex_separator(g, &b);
    let has = |s: u8| side.contains(&s);
    (has(0) && has(1)).then_some(side)
}

/// Minimum-degree order of a small graph (local indices): the vertex of
/// least current degree (ties: the smaller index) is eliminated and its
/// neighbours become a clique.
fn minimum_degree(g: &Graph) -> Vec<usize> {
    let k = g.n();
    let words = k.div_ceil(64).max(1);
    let mut bits = vec![0u64; k * words];
    for v in 0..k {
        for e in g.nbrs(v) {
            let u = g.adj[e];
            bits[v * words + u / 64] |= 1 << (u % 64);
        }
    }
    let mut alive = vec![0u64; words];
    for v in 0..k {
        alive[v / 64] |= 1 << (v % 64);
    }
    let mut order = Vec::with_capacity(k);
    let mut nb = vec![0u64; words];
    for _ in 0..k {
        let mut best = (usize::MAX, NONE);
        for v in 0..k {
            if alive[v / 64] >> (v % 64) & 1 == 0 {
                continue;
            }
            let d: usize = (0..words)
                .map(|w| (bits[v * words + w] & alive[w]).count_ones() as usize)
                .sum();
            if d < best.0 {
                best = (d, v);
            }
        }
        let v = best.1;
        order.push(v);
        alive[v / 64] &= !(1 << (v % 64));
        for w in 0..words {
            nb[w] = bits[v * words + w] & alive[w];
        }
        for w in 0..words {
            let mut m = nb[w];
            while m != 0 {
                let u = w * 64 + m.trailing_zeros() as usize;
                m &= m - 1;
                for x in 0..words {
                    bits[u * words + x] |= nb[x];
                }
                bits[u * words + u / 64] &= !(1 << (u % 64));
            }
        }
    }
    order
}

/// Appends the nested-dissection order of `verts` (ascending compressed
/// vertices) to `out`.
fn dissect(g: &Graph, verts: &[usize], loc: &mut [usize], out: &mut Vec<usize>) {
    if verts.is_empty() {
        return;
    }
    let sg = subgraph(g, verts, loc);
    let comps = components(&sg);
    if comps.len() > 1 {
        drop(sg);
        for c in comps {
            let sub: Vec<usize> = c.iter().map(|&l| verts[l]).collect();
            dissect(g, &sub, loc, out);
        }
        return;
    }
    if sg.n() <= LEAF {
        out.extend(minimum_degree(&sg).into_iter().map(|l| verts[l]));
        return;
    }
    match bisect(&sg) {
        Some(side) => {
            drop(sg);
            let pick = |s: u8| -> Vec<usize> {
                verts
                    .iter()
                    .zip(&side)
                    .filter(|&(_, &x)| x == s)
                    .map(|(&v, _)| v)
                    .collect()
            };
            let (p0, p1, sep) = (pick(0), pick(1), pick(2));
            dissect(g, &p0, loc, out);
            dissect(g, &p1, loc, out);
            out.extend(sep);
        }
        None if sg.n() <= MD_FALLBACK_MAX => {
            out.extend(minimum_degree(&sg).into_iter().map(|l| verts[l]));
        }
        None => out.extend_from_slice(verts),
    }
}

/// The nested-dissection elimination order of the columns of the `n × n`
/// pattern (`order[k]`: the original column eliminated `k`-th).
pub(crate) fn nested_dissection_order(
    n: usize,
    col_ptr: &[usize],
    row_idx: &[usize],
) -> Vec<usize> {
    let (xadj, adj) = symmetric_pattern(n, col_ptr, row_idx);
    Compressed::new(n, &xadj, &adj).order()
}

/// The supervariable graph of a symmetric pattern, the input of the
/// dissection (computed once, so that `auto` can look at its size before
/// paying for the dissection).
pub(crate) struct Compressed {
    g: Graph,
    members: Vec<Vec<usize>>,
    n: usize,
}

impl Compressed {
    /// Merges the vertices of equal closed neighbourhood of the pattern
    /// (`xadj`, `adj`) of order `n`.
    pub(crate) fn new(n: usize, xadj: &[usize], adj: &[usize]) -> Self {
        let (g, members) = compress(n, xadj, adj);
        Self { g, members, n }
    }

    /// Edges of the compressed graph.
    pub(crate) fn edges(&self) -> usize {
        self.g.adj.len() / 2
    }

    /// The nested-dissection elimination order of the original columns.
    pub(crate) fn order(&self) -> Vec<usize> {
        let all: Vec<usize> = (0..self.g.n()).collect();
        let mut loc = vec![NONE; self.g.n()];
        let mut corder = Vec::with_capacity(self.g.n());
        dissect(&self.g, &all, &mut loc, &mut corder);
        let mut order = Vec::with_capacity(self.n);
        for c in corder {
            order.extend_from_slice(&self.members[c]);
        }
        order
    }
}

/// Predicted size of the factors for the elimination order `order` of the
/// symmetric pattern (`xadj`, `adj`): `(Σ c_j, Σ c_j²)` over the column
/// counts `c_j` (diagonal included) of the Cholesky factor of the
/// permuted `A + Aᵀ` (elimination tree, then the column counts of
/// Gilbert, Ng and Peyton from the row subtrees' skeletons; almost linear
/// in the nonzeros). With diagonal pivots the LU factors of a structurally symmetric
/// matrix have this structure in both triangles.
pub(crate) fn cholesky_counts(xadj: &[usize], adj: &[usize], order: &[usize]) -> (f64, f64) {
    let n = order.len();
    let mut pinv = vec![0usize; n];
    for (k, &v) in order.iter().enumerate() {
        pinv[v] = k;
    }
    // elimination tree (Liu, with path compression through `anc`)
    let mut parent = vec![NONE; n];
    let mut anc = vec![NONE; n];
    for (k, &v) in order.iter().enumerate() {
        for &x in &adj[xadj[v]..xadj[v + 1]] {
            let mut i = pinv[x];
            while i != NONE && i < k {
                let next = anc[i];
                anc[i] = k;
                if next == NONE {
                    parent[i] = k;
                    break;
                }
                i = next;
            }
        }
    }
    // postorder of the tree (children in increasing order)
    let mut head = vec![NONE; n];
    let mut next = vec![NONE; n];
    for j in (0..n).rev() {
        if parent[j] != NONE {
            next[j] = head[parent[j]];
            head[parent[j]] = j;
        }
    }
    let mut post = Vec::with_capacity(n);
    let mut stack = Vec::new();
    for (root, &pr) in parent.iter().enumerate() {
        if pr != NONE {
            continue;
        }
        stack.push(root);
        while let Some(&j) = stack.last() {
            let c = head[j];
            if c == NONE {
                stack.pop();
                post.push(j);
            } else {
                head[j] = next[c];
                stack.push(c);
            }
        }
    }
    // column counts (Gilbert, Ng and Peyton): the skeleton of each row
    // subtree through first descendants and least common ancestors
    // (`ancestor` as a disjoint-set forest with path compression)
    let mut first = vec![NONE; n];
    let mut delta = vec![0i64; n];
    for (k, &j) in post.iter().enumerate() {
        delta[j] = i64::from(first[j] == NONE);
        let mut q = j;
        while q != NONE && first[q] == NONE {
            first[q] = k;
            q = parent[q];
        }
    }
    let mut ancestor: Vec<usize> = (0..n).collect();
    let mut maxfirst = vec![NONE; n];
    let mut prevleaf = vec![NONE; n];
    for &j in &post {
        if parent[j] != NONE {
            delta[parent[j]] -= 1;
        }
        let v = order[j];
        for &x in &adj[xadj[v]..xadj[v + 1]] {
            let i = pinv[x];
            // row i's subtree has j as a leaf when j is not inside the
            // subtree of the previous leaf found for i
            if i <= j || (maxfirst[i] != NONE && first[j] <= maxfirst[i]) {
                continue;
            }
            maxfirst[i] = first[j];
            let jprev = prevleaf[i];
            prevleaf[i] = j;
            delta[j] += 1;
            if jprev != NONE {
                let mut q = jprev;
                while q != ancestor[q] {
                    q = ancestor[q];
                }
                let mut s = jprev;
                while s != q {
                    let sp = ancestor[s];
                    ancestor[s] = q;
                    s = sp;
                }
                delta[q] -= 1;
            }
        }
        if parent[j] != NONE {
            ancestor[j] = parent[j];
        }
    }
    for &j in &post {
        if parent[j] != NONE {
            delta[parent[j]] += delta[j];
        }
    }
    let nnz: f64 = delta.iter().map(|&c| c as f64).sum();
    let flops: f64 = delta.iter().map(|&c| (c as f64) * (c as f64)).sum();
    (nnz, flops)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The counts by walking every row subtree (`O(|L|)`), the reference.
    fn cholesky_counts_by_row_subtrees(
        xadj: &[usize],
        adj: &[usize],
        order: &[usize],
    ) -> (f64, f64) {
        let n = order.len();
        let mut pinv = vec![0usize; n];
        for (k, &v) in order.iter().enumerate() {
            pinv[v] = k;
        }
        // elimination tree (Liu, with path compression through `anc`)
        let mut parent = vec![NONE; n];
        let mut anc = vec![NONE; n];
        for (k, &v) in order.iter().enumerate() {
            for &x in &adj[xadj[v]..xadj[v + 1]] {
                let mut i = pinv[x];
                if i >= k {
                    continue;
                }
                while i != NONE && i < k {
                    let next = anc[i];
                    anc[i] = k;
                    if next == NONE {
                        parent[i] = k;
                        break;
                    }
                    i = next;
                }
            }
        }
        // column counts: row k of L is the union of the tree paths from its
        // nonzeros below the diagonal up to k
        let mut count = vec![1u64; n];
        let mut mark = vec![NONE; n];
        for k in 0..n {
            mark[k] = k;
            let v = order[k];
            for &x in &adj[xadj[v]..xadj[v + 1]] {
                let mut j = pinv[x];
                if j >= k {
                    continue;
                }
                while mark[j] != k {
                    count[j] += 1;
                    mark[j] = k;
                    j = parent[j];
                    if j == NONE {
                        break;
                    }
                }
            }
        }
        let nnz: f64 = count.iter().map(|&c| c as f64).sum();
        let flops: f64 = count.iter().map(|&c| (c as f64) * (c as f64)).sum();
        (nnz, flops)
    }

    /// The pattern of a 3-D grid, `c` unknowns per node, 26 neighbours.
    pub(crate) fn grid(p: usize, c: usize) -> (usize, Vec<usize>, Vec<usize>) {
        let node = |i: usize, j: usize, k: usize| (i * p + j) * p + k;
        let n = p * p * p * c;
        let mut cols: Vec<Vec<usize>> = vec![Vec::new(); n];
        for i in 0..p {
            for j in 0..p {
                for k in 0..p {
                    let a = node(i, j, k);
                    for di in -1i64..=1 {
                        for dj in -1i64..=1 {
                            for dk in -1i64..=1 {
                                let (x, y, z) = (i as i64 + di, j as i64 + dj, k as i64 + dk);
                                if x < 0 || y < 0 || z < 0 {
                                    continue;
                                }
                                let (x, y, z) = (x as usize, y as usize, z as usize);
                                if x >= p || y >= p || z >= p {
                                    continue;
                                }
                                let b = node(x, y, z);
                                for r in 0..c {
                                    for s in 0..c {
                                        cols[b * c + s].push(a * c + r);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        let mut cp = vec![0];
        let mut ri = Vec::new();
        for mut col in cols {
            col.sort_unstable();
            ri.extend(col);
            cp.push(ri.len());
        }
        (n, cp, ri)
    }

    fn is_perm(order: &[usize], n: usize) -> bool {
        let mut seen = vec![false; n];
        order.len() == n
            && order.iter().all(|&v| {
                let fresh = v < n && !seen[v];
                if fresh {
                    seen[v] = true;
                }
                fresh
            })
    }

    #[test]
    fn order_is_a_deterministic_permutation() {
        let (n, cp, ri) = grid(9, 3);
        let a = nested_dissection_order(n, &cp, &ri);
        assert!(is_perm(&a, n));
        assert_eq!(a, nested_dissection_order(n, &cp, &ri));
        // the three unknowns of a node stay together
        for t in a.chunks(3) {
            assert_eq!(t[0] / 3, t[2] / 3);
        }
    }

    #[test]
    fn fill_is_below_the_natural_order_on_a_3d_grid() {
        let (n, cp, ri) = grid(12, 1);
        let (x, a) = symmetric_pattern(n, &cp, &ri);
        let nd = nested_dissection_order(n, &cp, &ri);
        let natural: Vec<usize> = (0..n).collect();
        let (nnz_nd, fl_nd) = cholesky_counts(&x, &a, &nd);
        let (nnz_nat, fl_nat) = cholesky_counts(&x, &a, &natural);
        assert!(nnz_nd < 0.7 * nnz_nat, "{nnz_nd} vs {nnz_nat}");
        assert!(fl_nd < 0.5 * fl_nat, "{fl_nd} vs {fl_nat}");
    }

    #[test]
    fn small_disconnected_and_empty_patterns() {
        assert!(nested_dissection_order(0, &[0], &[]).is_empty());
        // diagonal only: no edges
        let cp: Vec<usize> = (0..=5).collect();
        let ri: Vec<usize> = (0..5).collect();
        assert_eq!(nested_dissection_order(5, &cp, &ri), vec![0, 1, 2, 3, 4]);
        // two disjoint grids
        let (n, cp, ri) = grid(6, 1);
        let mut cp2 = cp.clone();
        let mut ri2 = ri.clone();
        for j in 0..n {
            for &i in &ri[cp[j]..cp[j + 1]] {
                ri2.push(i + n);
            }
            cp2.push(ri2.len());
        }
        let o = nested_dissection_order(2 * n, &cp2, &ri2);
        assert!(is_perm(&o, 2 * n));
        // the first component is ordered entirely before the second
        assert!(o[..n].iter().all(|&v| v < n));
    }

    #[test]
    fn cholesky_counts_of_a_tridiagonal_and_an_arrow() {
        // path 0-1-2-3: no fill, counts 2,2,2,1
        let x = vec![0, 1, 3, 5, 6];
        let a = vec![1, 0, 2, 1, 3, 2];
        assert_eq!(cholesky_counts(&x, &a, &[0, 1, 2, 3]), (7.0, 13.0));
        // star with centre 0 eliminated first: complete fill (4,3,2,1)
        let x = vec![0, 3, 4, 5, 6];
        let a = vec![1, 2, 3, 0, 0, 0];
        assert_eq!(cholesky_counts(&x, &a, &[0, 1, 2, 3]), (10.0, 30.0));
        // centre last: no fill
        assert_eq!(cholesky_counts(&x, &a, &[1, 2, 3, 0]), (7.0, 13.0));
    }

    #[test]
    fn the_separator_separates_and_is_small() {
        for (p, c) in [(10, 1), (8, 3), (16, 1)] {
            let (n, cp, ri) = grid(p, c);
            let (x, a) = symmetric_pattern(n, &cp, &ri);
            let (g, _) = compress(n, &x, &a);
            let side = bisect(&g).expect("a proper split");
            for v in 0..g.n() {
                for k in g.nbrs(v) {
                    let u = g.adj[k];
                    assert!(
                        !(side[v] == 0 && side[u] == 1),
                        "edge {v}-{u} crosses the separator"
                    );
                }
            }
            let w = |s: u8| -> u64 {
                (0..g.n())
                    .filter(|&v| side[v] == s)
                    .map(|v| g.vwgt[v])
                    .sum()
            };
            let (w0, w1, ws) = (w(0), w(1), w(2));
            // a plane of the grid has p² nodes; the cover of a cut through a
            // 26-neighbour grid needs at most about two planes
            assert!(ws <= (2 * p * p * c) as u64, "separator {ws} for p {p}");
            assert!(
                w0.max(w1) <= (0.62 * (w0 + w1 + ws) as f64) as u64,
                "{w0} {w1} {ws}"
            );
        }
    }

    #[test]
    fn column_counts_equal_the_row_subtree_walk() {
        for (p, c) in [(6, 1), (5, 3), (9, 1)] {
            let (n, cp, ri) = grid(p, c);
            let (x, a) = symmetric_pattern(n, &cp, &ri);
            let nd = nested_dissection_order(n, &cp, &ri);
            let natural: Vec<usize> = (0..n).collect();
            // a scrambled order (deterministic)
            let mut scrambled = natural.clone();
            scrambled.sort_by_key(|&v| (v * 7919) % n);
            for order in [&nd, &natural, &scrambled] {
                assert_eq!(
                    cholesky_counts(&x, &a, order),
                    cholesky_counts_by_row_subtrees(&x, &a, order)
                );
            }
        }
        // a forest (two components) and a matrix with isolated vertices
        let x = vec![0, 1, 2, 2, 3, 4];
        let a = vec![1, 0, 4, 3];
        for order in [vec![0, 1, 2, 3, 4], vec![4, 2, 0, 3, 1]] {
            assert_eq!(
                cholesky_counts(&x, &a, &order),
                cholesky_counts_by_row_subtrees(&x, &a, &order)
            );
        }
    }

    #[test]
    fn minimum_degree_eliminates_leaves_of_a_path_first() {
        let g = Graph {
            xadj: vec![0, 1, 3, 5, 6],
            adj: vec![1, 0, 2, 1, 3, 2],
            ewgt: vec![1; 6],
            vwgt: vec![1; 4],
        };
        assert_eq!(minimum_degree(&g), vec![0, 1, 2, 3]);
    }
}
