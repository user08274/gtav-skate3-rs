//! Grindable edges found in GTA's ground: curbs, ledges and step noses.
//!
//! GTA only answers rays, so the finder keeps a world-aligned grid of
//! downward samples around the skater. Neighbouring samples whose heights
//! differ by a ledge-like step are refined by bisection with a few more rays
//! until the step is located to under a centimetre; a slope fails that test
//! (its midpoints are neither height). The edge points along the top corner
//! are then joined into polylines, which become Skate 3 grind splines.
use crate::{coords::GtaVec, terrain::GroundProbe};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug)]
pub struct EdgeSettings {
    /// Grid spacing of the coarse samples, metres.
    pub spacing: f32,
    /// Sampling radius around the skater, metres.
    pub radius: f32,
    /// Height differences treated as a grindable step (curb up to waist-high ledge).
    pub min_step: f32,
    pub max_step: f32,
    /// Rays allowed per update.
    pub rays: usize,
    /// Bisection steps (spacing / 2^steps is the edge precision).
    pub refine_steps: u32,
}

impl Default for EdgeSettings {
    fn default() -> Self {
        Self { spacing: 0.5, radius: 6.0, min_step: 0.06, max_step: 1.3, rays: 64, refine_steps: 6 }
    }
}

/// A point on the top corner of a step and the horizontal direction down it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgePoint {
    pub position: GtaVec,
    pub drop: GtaVec,
}

type Node = (i32, i32);

pub struct EdgeFinder {
    pub settings: EdgeSettings,
    heights: HashMap<Node, Option<f32>>,
    checked: HashSet<(Node, u8)>,
    points: Vec<EdgePoint>,
    /// Bumped whenever an edge point is added.
    pub version: u64,
}

fn length2(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

impl EdgeFinder {
    pub fn new(settings: EdgeSettings) -> Self {
        Self { settings, heights: HashMap::new(), checked: HashSet::new(), points: Vec::new(), version: 0 }
    }

    pub fn points(&self) -> &[EdgePoint] {
        &self.points
    }

    fn world(&self, node: Node) -> (f32, f32) {
        (node.0 as f32 * self.settings.spacing, node.1 as f32 * self.settings.spacing)
    }

    fn ray(probe: &mut dyn GroundProbe, x: f32, y: f32, around: f32) -> Option<f32> {
        probe.down(x, y, around + 3.0, around - 4.0)
    }

    /// Samples and refines around `center` within the ray budget; returns
    /// whether new edge points were found.
    pub fn update(&mut self, probe: &mut dyn GroundProbe, center: GtaVec) -> bool {
        let s = self.settings;
        let mut rays = 0usize;
        let reach = (s.radius / s.spacing).ceil() as i32;
        let (cx, cy) = ((center.x / s.spacing).round() as i32, (center.y / s.spacing).round() as i32);
        // Nearest nodes first.
        let mut nodes: Vec<Node> = (-reach..=reach)
            .flat_map(|dx| (-reach..=reach).map(move |dy| (dx, dy)))
            .filter(|&(dx, dy)| (dx * dx + dy * dy) as f32 <= (reach * reach) as f32)
            .map(|(dx, dy)| (cx + dx, cy + dy))
            .collect();
        nodes.sort_by_key(|&(x, y)| (x - cx).pow(2) + (y - cy).pow(2));
        for &node in &nodes {
            if rays >= s.rays / 2 {
                break;
            }
            if !self.heights.contains_key(&node) {
                let (x, y) = self.world(node);
                let h = Self::ray(probe, x, y, center.z);
                self.heights.insert(node, h);
                rays += 1;
            }
        }
        let before = self.points.len();
        for &node in &nodes {
            for dir in 0..2u8 {
                if self.checked.contains(&(node, dir)) {
                    continue;
                }
                let other = if dir == 0 { (node.0 + 1, node.1) } else { (node.0, node.1 + 1) };
                let (Some(&a), Some(&b)) = (self.heights.get(&node), self.heights.get(&other)) else { continue };
                let step = match (a, b) {
                    (Some(a), Some(b)) => (a - b).abs(),
                    _ => 0.0,
                };
                if step < s.min_step || step > s.max_step {
                    self.checked.insert((node, dir));
                    continue;
                }
                if rays + s.refine_steps as usize > s.rays {
                    continue;
                }
                self.checked.insert((node, dir));
                let (a, b) = (a.unwrap(), b.unwrap());
                let (high, low, hh, hl) = if a > b { (node, other, a, b) } else { (other, node, b, a) };
                rays += s.refine_steps as usize;
                if let Some(point) = self.refine(probe, high, low, hh, hl, center.z) {
                    self.points.push(point);
                }
            }
        }
        // Forget samples far behind so memory stays bounded.
        if self.heights.len() > 40_000 {
            let keep = (reach * 3) as i64;
            let far = |&(x, y): &Node| ((x - cx) as i64).abs() > keep || ((y - cy) as i64).abs() > keep;
            self.heights.retain(|n, _| !far(n));
            self.checked.retain(|(n, _)| !far(n));
            let limit = s.radius * 3.0;
            self.points.retain(|p| length2(p.position.x - center.x, p.position.y - center.y) < limit);
            self.version += 1;
        }
        if self.points.len() > before {
            self.version += 1;
            true
        } else {
            false
        }
    }

    /// Bisection between a high and a low sample: the step must switch from
    /// one height straight to the other.
    fn refine(&self, probe: &mut dyn GroundProbe, high: Node, low: Node, hh: f32, hl: f32, around: f32) -> Option<EdgePoint> {
        let (a, b) = (self.world(high), self.world(low));
        let tolerance = ((hh - hl) / 3.0).min(0.03);
        let (mut t_high, mut t_low) = (0.0f32, 1.0f32);
        for _ in 0..self.settings.refine_steps {
            let t = (t_high + t_low) * 0.5;
            let h = Self::ray(probe, a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, around)?;
            if (h - hh).abs() <= tolerance {
                t_high = t;
            } else if (h - hl).abs() <= tolerance {
                t_low = t;
            } else {
                return None;
            }
        }
        let len = length2(b.0 - a.0, b.1 - a.1);
        let drop = GtaVec::new((b.0 - a.0) / len, (b.1 - a.1) / len, 0.0);
        Some(EdgePoint { position: GtaVec::new(a.0 + (b.0 - a.0) * t_high, a.1 + (b.1 - a.1) * t_high, hh), drop })
    }

    /// Edge points near `center` joined into polylines along the step tops.
    pub fn polylines(&self, center: GtaVec, radius: f32) -> Vec<Vec<GtaVec>> {
        let link = self.settings.spacing * 1.6;
        let near: Vec<&EdgePoint> = self
            .points
            .iter()
            .filter(|p| length2(p.position.x - center.x, p.position.y - center.y) <= radius)
            .collect();
        // Union-find over points close in plan and height, dropping the same way.
        let mut parent: Vec<usize> = (0..near.len()).collect();
        fn root(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        for i in 0..near.len() {
            for j in i + 1..near.len() {
                let (p, q) = (near[i], near[j]);
                let close = length2(p.position.x - q.position.x, p.position.y - q.position.y) <= link
                    && (p.position.z - q.position.z).abs() <= 0.05
                    && p.drop.x * q.drop.x + p.drop.y * q.drop.y > 0.0;
                if close {
                    let (a, b) = (root(&mut parent, i), root(&mut parent, j));
                    parent[a] = b;
                }
            }
        }
        let mut groups: HashMap<usize, Vec<GtaVec>> = HashMap::new();
        for i in 0..near.len() {
            let r = root(&mut parent, i);
            groups.entry(r).or_default().push(near[i].position);
        }
        let mut lines = Vec::new();
        let mut keys: Vec<usize> = groups.keys().copied().collect();
        keys.sort_unstable();
        for key in keys {
            let points = &groups[&key];
            if points.len() < 3 {
                continue;
            }
            for piece in order_along(points, link) {
                let length: f32 = piece.windows(2).map(|w| length2(w[1].x - w[0].x, w[1].y - w[0].y)).sum();
                if piece.len() >= 3 && length >= 1.0 {
                    lines.push(simplify(&piece, 0.03));
                }
            }
        }
        lines
    }
}

/// Orders points along their main direction and splits at gaps and sharp turns.
fn order_along(points: &[GtaVec], gap: f32) -> Vec<Vec<GtaVec>> {
    let n = points.len() as f32;
    let (mx, my) = (points.iter().map(|p| p.x).sum::<f32>() / n, points.iter().map(|p| p.y).sum::<f32>() / n);
    let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
    for p in points {
        let (dx, dy) = (p.x - mx, p.y - my);
        sxx += dx * dx;
        sxy += dx * dy;
        syy += dy * dy;
    }
    // Principal axis of the 2x2 covariance.
    let angle = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    let (ux, uy) = (angle.cos(), angle.sin());
    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| (a.x * ux + a.y * uy).total_cmp(&(b.x * ux + b.y * uy)));
    let mut pieces: Vec<Vec<GtaVec>> = vec![Vec::new()];
    for p in sorted {
        let piece = pieces.last_mut().unwrap();
        let split = match piece.as_slice() {
            [.., a, b] => {
                let (d1x, d1y) = (b.x - a.x, b.y - a.y);
                let (d2x, d2y) = (p.x - b.x, p.y - b.y);
                let turn = (d1x * d2x + d1y * d2y) / (length2(d1x, d1y) * length2(d2x, d2y)).max(1e-6);
                length2(d2x, d2y) > gap || turn < 0.5
            }
            [b] => length2(p.x - b.x, p.y - b.y) > gap,
            [] => false,
        };
        if split {
            pieces.push(Vec::new());
        }
        pieces.last_mut().unwrap().push(p);
    }
    pieces
}

/// Ramer-Douglas-Peucker in plan view (heights follow the kept points).
fn simplify(points: &[GtaVec], tolerance: f32) -> Vec<GtaVec> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let (a, b) = (points[0], points[points.len() - 1]);
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = length2(dx, dy).max(1e-6);
    let mut worst = (0usize, 0.0f32);
    for (i, p) in points.iter().enumerate().take(points.len() - 1).skip(1) {
        let d = ((p.x - a.x) * dy - (p.y - a.y) * dx).abs() / len;
        let dz = {
            let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / (len * len)).clamp(0.0, 1.0);
            (p.z - (a.z + (b.z - a.z) * t)).abs()
        };
        let d = d.max(dz);
        if d > worst.1 {
            worst = (i, d);
        }
    }
    if worst.1 <= tolerance {
        return vec![a, b];
    }
    let mut left = simplify(&points[..=worst.0], tolerance);
    let right = simplify(&points[worst.0..], tolerance);
    left.pop();
    left.extend(right);
    left
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 15 cm curb along y = 3.3 (high side north), and a gentle ramp east.
    struct Street;
    impl GroundProbe for Street {
        fn down(&mut self, x: f32, y: f32, top: f32, bottom: f32) -> Option<f32> {
            let z = if y >= 3.3 && x < 4.0 { 0.15 } else if x >= 4.0 { (x - 4.0) * 0.3 } else { 0.0 };
            (top >= z && bottom <= z).then_some(z)
        }
    }

    #[test]
    fn a_curb_becomes_one_straight_line_and_a_slope_none() {
        let mut finder = EdgeFinder::new(EdgeSettings { rays: 100_000, ..EdgeSettings::default() });
        let center = GtaVec::new(0.0, 0.0, 0.0);
        assert!(finder.update(&mut Street, center));
        let lines = finder.polylines(center, 6.0);
        assert_eq!(lines.len(), 1, "{lines:?}");
        let line = &lines[0];
        assert_eq!(line.len(), 2, "straight curb simplifies to its ends: {line:?}");
        for p in line {
            assert!((p.y - 3.3).abs() < 0.01, "edge located to a centimetre: {p:?}");
            assert!((p.z - 0.15).abs() < 1e-4);
            assert!(p.x < 4.0);
        }
        assert!((line[0].x - line[1].x).abs() > 4.0, "runs along the curb: {line:?}");
        // The curb's east end, where it meets the ramp's foot, is a real step too.
        for p in finder.points().iter().filter(|p| p.position.x < 3.9) {
            assert!(p.drop.y < -0.9, "drops south: {p:?}");
        }
    }

    #[test]
    fn the_ray_budget_spreads_work_over_updates() {
        let mut finder = EdgeFinder::new(EdgeSettings::default());
        let center = GtaVec::new(0.0, 0.0, 0.0);
        let mut updates = 0;
        while finder.polylines(center, 6.0).is_empty() {
            finder.update(&mut Street, center);
            updates += 1;
            assert!(updates < 100);
        }
        assert!(updates > 3, "more than one update's worth of rays: {updates}");
    }
}
