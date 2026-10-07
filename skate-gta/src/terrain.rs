//! Local collision patch around the board. GTA only answers ray probes, while
//! the Skate 3 contact pipeline needs triangles, so a grid of downward probes
//! is triangulated into a small height field and handed to `BoardWorld`.
use crate::coords::{Frame, GtaVec};
use skate_core::{
    math::Vector3,
    physics::{
        board_world::{
            BoardWorld, WorldTriangle,
            query_metadata::{Bounds, QueryMesh, QueryMetadata, QueryPool},
        },
        collision::TriangleFeature,
        contact::RetailContactMaterial,
        drive_frames::RetailAffineTransform,
        world_contact::triangle_from_volume,
    },
};

pub trait GroundProbe {
    /// First surface hit on the vertical segment from `top` down to `bottom`.
    fn down(&mut self, x: f32, y: f32, top: f32, bottom: f32) -> Option<f32>;
}

#[derive(Clone, Copy, Debug)]
pub struct PatchSettings {
    pub cells: usize,
    pub spacing: f32,
    pub probe_above: f32,
    pub probe_below: f32,
    pub resample_distance: f32,
    pub resample_ticks: u32,
}

impl Default for PatchSettings {
    fn default() -> Self {
        Self {
            cells: 8,
            spacing: 0.4,
            probe_above: 1.5,
            probe_below: 3.0,
            resample_distance: 0.4,
            resample_ticks: 15,
        }
    }
}

pub struct Patch {
    pub center: GtaVec,
    pub samples: Vec<Option<GtaVec>>,
    pub triangles: Vec<WorldTriangle>,
    pub age_ticks: u32,
}

impl Patch {
    pub fn needs_resample(&self, deck: GtaVec, settings: &PatchSettings) -> bool {
        let d = deck.sub(self.center);
        self.age_ticks >= settings.resample_ticks
            || (d.x * d.x + d.y * d.y).sqrt() > settings.resample_distance
            || d.z.abs() > settings.resample_distance
    }
}

pub fn sample(
    probe: &mut dyn GroundProbe,
    frame: &Frame,
    center: GtaVec,
    settings: &PatchSettings,
    material: RetailContactMaterial,
) -> Patch {
    let n = settings.cells + 1;
    let half = settings.cells as f32 * settings.spacing * 0.5;
    let mut samples = Vec::with_capacity(n * n);
    for j in 0..n {
        for i in 0..n {
            let x = center.x - half + i as f32 * settings.spacing;
            let y = center.y - half + j as f32 * settings.spacing;
            let hit = probe.down(
                x,
                y,
                center.z + settings.probe_above,
                center.z - settings.probe_below,
            );
            samples.push(hit.map(|z| GtaVec::new(x, y, z)));
        }
    }
    let mut triangles = Vec::new();
    let mut emit = |i: usize, j: usize, size: usize| {
        let corner = |di: usize, dj: usize| samples[(j + dj) * n + i + di];
        let (Some(a), Some(b), Some(c), Some(d)) =
            (corner(0, 0), corner(size, 0), corner(size, size), corner(0, size))
        else {
            return;
        };
        for tri in [[a, b, c], [a, c, d]] {
            if let Some(t) = upward_triangle(tri.map(|p| frame.to_skate(p)), material) {
                triangles.push(t);
            }
        }
    };
    if settings.cells.is_power_of_two() {
        merge_planar(&samples, n, 0, 0, settings.cells, &mut emit);
    } else {
        for j in 0..settings.cells {
            for i in 0..settings.cells {
                emit(i, j, 1);
            }
        }
    }
    Patch {
        center,
        samples,
        triangles,
        age_ticks: 0,
    }
}

/// Samples further than this from a block's corner plane keep it subdivided.
const PLANAR_TOLERANCE: f32 = 0.005;

/// Quadtree over the sample grid: a block whose samples all lie on the plane
/// through its corners becomes two triangles, otherwise its quarters are
/// visited. Every contact row costs solver time, and flat GTA ground would
/// otherwise produce dozens of redundant seam contacts. Midpoints of a merged
/// block lie on its edges, so neighbouring finer blocks leave no cracks.
fn merge_planar(
    samples: &[Option<GtaVec>],
    n: usize,
    i: usize,
    j: usize,
    size: usize,
    emit: &mut impl FnMut(usize, usize, usize),
) {
    if size == 1 || block_is_planar(samples, n, i, j, size) {
        emit(i, j, size);
        return;
    }
    let half = size / 2;
    for (di, dj) in [(0, 0), (half, 0), (0, half), (half, half)] {
        merge_planar(samples, n, i + di, j + dj, half, emit);
    }
}

fn block_is_planar(samples: &[Option<GtaVec>], n: usize, i: usize, j: usize, size: usize) -> bool {
    let at = |di: usize, dj: usize| samples[(j + dj) * n + i + di];
    let (Some(a), Some(b), Some(d)) = (at(0, 0), at(size, 0), at(0, size)) else {
        return false;
    };
    // z = a.z + gx * (x - a.x) + gy * (y - a.y) on a regular grid.
    let gx = (b.z - a.z) / (b.x - a.x);
    let gy = (d.z - a.z) / (d.y - a.y);
    (0..=size).all(|dj| {
        (0..=size).all(|di| {
            at(di, dj).is_some_and(|p| {
                let z = a.z + gx * (p.x - a.x) + gy * (p.y - a.y);
                (p.z - z).abs() <= PLANAR_TOLERANCE
            })
        })
    })
}

/// One-sided triangle whose winding gives an upward (+Y) face normal, matching
/// the convention of the original level geometry: normal = (b - a) x (c - a).
fn upward_triangle(v: [Vector3; 3], material: RetailContactMaterial) -> Option<WorldTriangle> {
    let e1 = Vector3::new(v[1].x - v[0].x, v[1].y - v[0].y, v[1].z - v[0].z);
    let e2 = Vector3::new(v[2].x - v[0].x, v[2].y - v[0].y, v[2].z - v[0].z);
    let ny = e1.z * e2.x - e1.x * e2.z;
    let ordered = if ny >= 0.0 { v } else { [v[0], v[2], v[1]] };
    let triangle = triangle_from_volume(ordered, 0.0, [1.0; 3], TriangleFeature::ONE_SIDED);
    let n = triangle.feature.normal;
    let valid = triangle.edge_lengths.iter().all(|l| l.is_finite() && *l > 0.0)
        && n.x.is_finite()
        && n.y.is_finite()
        && n.z.is_finite()
        && n.y > 0.0;
    valid.then_some(WorldTriangle {
        triangle,
        material,
        tag: 0,
    })
}

/// Static ground mesh with per-triangle bounds, as the original host builds
/// its levels, so contact queries only visit triangles near each volume.
pub fn world(patch: &Patch) -> BoardWorld {
    world_of(std::slice::from_ref(patch))
}

pub fn world_of(patches: &[Patch]) -> BoardWorld {
    let triangles: Vec<_> = patches.iter().flat_map(|p| p.triangles.iter().copied()).collect();
    let Some(local_bounds) =
        Bounds::from_points(triangles.iter().flat_map(|t| t.triangle.vertices))
    else {
        return BoardWorld::new(triangles);
    };
    let metadata = QueryMetadata {
        packed_surfaces: vec![0u16; triangles.len()],
        meshes: vec![QueryMesh {
            triangle_range: 0..triangles.len(),
            local_to_world: RetailAffineTransform::IDENTITY,
            world_to_local: RetailAffineTransform::IDENTITY,
            local_bounds,
            matching_group: -1,
            rejection_flags: 0,
            geometry: 1,
            pool: QueryPool::Ground,
        }],
        static_edges: Vec::new(),
        island_flags: 0,
    };
    BoardWorld::with_query_metadata(triangles.clone(), metadata)
        .unwrap_or_else(|_| BoardWorld::new(triangles))
}

/// Ground patches that follow several moving centres (board and skater).
/// Centres closer than one patch share a patch at their midpoint.
pub struct Patches {
    pub settings: PatchSettings,
    pub material: RetailContactMaterial,
    pub patches: Vec<Patch>,
}

impl Patches {
    pub fn new(settings: PatchSettings, material: RetailContactMaterial) -> Self {
        Self { settings, material, patches: Vec::new() }
    }

    fn half_extent(&self) -> f32 {
        self.settings.cells as f32 * self.settings.spacing * 0.5
    }

    /// Returns true when the patch set changed and the world must be rebuilt.
    pub fn refresh(&mut self, probe: &mut dyn GroundProbe, frame: &Frame, centers: &[GtaVec]) -> bool {
        let wanted = self.cluster(centers);
        let stale = wanted.len() != self.patches.len()
            || wanted.iter().zip(&self.patches).any(|(c, p)| p.needs_resample(*c, &self.settings));
        if stale {
            self.patches = wanted
                .iter()
                .map(|c| sample(probe, frame, *c, &self.settings, self.material))
                .collect();
        } else {
            for p in &mut self.patches {
                p.age_ticks += 1;
            }
        }
        stale
    }

    fn cluster(&self, centers: &[GtaVec]) -> Vec<GtaVec> {
        let reach = self.half_extent();
        let mut out: Vec<GtaVec> = Vec::new();
        for &c in centers {
            if let Some(near) = out.iter_mut().find(|o| {
                let d = o.sub(c);
                (d.x * d.x + d.y * d.y).sqrt() < reach
            }) {
                *near = near.add(c).scale(0.5);
            } else {
                out.push(c);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Slope;
    impl GroundProbe for Slope {
        fn down(&mut self, x: f32, _y: f32, top: f32, bottom: f32) -> Option<f32> {
            let z = 10.0 + 0.25 * x;
            (z <= top && z >= bottom).then_some(z)
        }
    }

    struct Hole;
    impl GroundProbe for Hole {
        fn down(&mut self, x: f32, y: f32, _top: f32, _bottom: f32) -> Option<f32> {
            (x.abs() > 0.5 || y.abs() > 0.5).then_some(0.0)
        }
    }

    const MATERIAL: RetailContactMaterial = RetailContactMaterial {
        static_friction: 0.0,
        dynamic_friction: 0.0,
        restitution: 1.0,
    };

    #[test]
    fn slope_becomes_upward_triangles_in_skate_space() {
        let frame = Frame::new(GtaVec::new(0.0, 0.0, 10.0));
        let settings = PatchSettings::default();
        let patch = sample(&mut Slope, &frame, GtaVec::new(0.0, 0.0, 10.0), &settings, MATERIAL);
        assert_eq!(patch.triangles.len(), 2, "a plane needs no subdivision");
        for t in &patch.triangles {
            let n = t.triangle.feature.normal;
            assert!(n.y > 0.9, "normal {n:?}");
            // GTA +X slope rises, so the normal leans toward skate -X.
            assert!(n.x < 0.0);
        }
    }

    struct Curb;
    impl GroundProbe for Curb {
        fn down(&mut self, x: f32, _y: f32, _top: f32, _bottom: f32) -> Option<f32> {
            Some(if x > 0.3 { 0.15 } else { 0.0 })
        }
    }

    #[test]
    fn a_curb_keeps_fine_triangles_only_along_its_edge() {
        let frame = Frame::new(GtaVec::default());
        let settings = PatchSettings::default();
        let patch = sample(&mut Curb, &frame, GtaVec::default(), &settings, MATERIAL);
        let full = settings.cells * settings.cells * 2;
        assert!(patch.triangles.len() > 2 && patch.triangles.len() < full / 2, "{}", patch.triangles.len());
        let steep = patch.triangles.iter().filter(|t| t.triangle.feature.normal.y < 0.99).count();
        assert!(steep > 0, "the curb face is represented");
    }

    #[test]
    fn missing_probes_leave_holes_instead_of_bridging() {
        let frame = Frame::new(GtaVec::default());
        let settings = PatchSettings::default();
        let patch = sample(&mut Hole, &frame, GtaVec::default(), &settings, MATERIAL);
        assert!(patch.triangles.len() < settings.cells * settings.cells * 2);
        assert!(!patch.triangles.is_empty());
    }

    #[test]
    fn resample_after_moving_or_ageing() {
        let frame = Frame::new(GtaVec::default());
        let settings = PatchSettings::default();
        let mut patch = sample(&mut Hole, &frame, GtaVec::default(), &settings, MATERIAL);
        assert!(!patch.needs_resample(GtaVec::new(0.1, 0.1, 0.0), &settings));
        assert!(patch.needs_resample(GtaVec::new(0.5, 0.0, 0.0), &settings));
        patch.age_ticks = settings.resample_ticks;
        assert!(patch.needs_resample(GtaVec::default(), &settings));
    }
}
