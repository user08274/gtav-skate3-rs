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
    },
};

pub trait GroundProbe {
    /// First surface hit on the vertical segment from `top` down to `bottom`.
    fn down(&mut self, x: f32, y: f32, top: f32, bottom: f32) -> Option<f32>;

    /// First surface hit on the segment `from` -> `to`: point and face normal.
    fn toward(&mut self, _from: GtaVec, _to: GtaVec) -> Option<(GtaVec, GtaVec)> {
        None
    }
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
            // Deep enough to find the ground under a high jump.
            probe_below: 50.0,
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
    // World-aligned grid: a resampled patch probes the same GTA points, so
    // the surface under the wheels does not shift between rebuilds.
    let snap = |v: f32| (v / settings.spacing).round() * settings.spacing;
    let center = GtaVec::new(snap(center.x), snap(center.y), center.z);
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
            ).or_else(|| probe.down(x,y,center.z+6.0,center.z-settings.probe_below));
            samples.push(hit.map(|z| GtaVec::new(x, y, z)));
        }
    }
    let mut faces: Vec<[usize; 3]> = Vec::new();
    let mut partials = Vec::new();
    let mut emit = |i: usize, j: usize, size: usize| {
        let id = |di: usize, dj: usize| (j + dj) * n + i + di;
        let quad = [id(0, 0), id(size, 0), id(size, size), id(0, size)];
        if quad.iter().any(|&k| samples[k].is_none()) {
            let valid: Vec<_> = quad.iter().copied().filter(|&k|samples[k].is_some()).collect();
            if valid.len()==3 {partials.push([valid[0],valid[1],valid[2]]);}
            return;
        }
        faces.push([quad[0], quad[1], quad[2]]);
        faces.push([quad[0], quad[2], quad[3]]);
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
    let triangles = build_surface(probe, &samples, faces, partials, frame, material);
    Patch {
        center,
        samples,
        triangles,
        age_ticks: 0,
    }
}

/// Locate an actual jump, including curbs on sloping pavement. Smooth slopes
/// lose their height difference as the interval shrinks; a step retains it.
pub(crate) fn step_between(probe: &mut dyn GroundProbe, start: GtaVec, end: GtaVec) -> Option<(GtaVec, GtaVec)> {
    let (mut a, mut b) = (start, end);
    let mut checked_bevel = false;
    for _ in 0..8 {
        if (a.z - b.z).abs() < 0.03 { return None; }
        let p = a.lerp(b, 0.5);
        let z = probe.down(p.x, p.y, a.z.max(b.z) + 0.5, a.z.min(b.z) - 1.5)?;
        let run = ((a.x-b.x).powi(2) + (a.y-b.y).powi(2)).sqrt();
        // A short steep bevel between two walkable surfaces is a curb face.
        // Test both plateaus so a continuous quarter-pipe remains a ramp.
        if !checked_bevel && run <= 0.08 && (a.z-b.z).abs() / run.max(1e-6) > 1.5
            && (a.z-z).abs()>0.006 && (b.z-z).abs()>0.006 {
            checked_bevel = true;
            let (high, low) = if a.z > b.z { (a,b) } else { (b,a) };
            let along = GtaVec::new((high.x-low.x)/run, (high.y-low.y)/run, 0.);
            let locations = [high.add(along.scale(0.1)), high.add(along.scale(0.15)),
                low.sub(along.scale(0.1)), low.sub(along.scale(0.15))];
            let heights: Vec<_> = locations.iter().map(|p|
                probe.down(p.x,p.y,high.z+0.5,low.z-1.5)).collect();
            if let [Some(h),Some(hf),Some(l),Some(lf)] = heights.as_slice() {
                if (hf-h).abs() < 0.025 && (l-lf).abs() < 0.025 {
                    let cut = high.lerp(low,0.5);
                    let offset = |p:GtaVec| (cut.x-p.x)*along.x + (cut.y-p.y)*along.y;
                    let hz = h + offset(locations[0])*(hf-h)/0.05;
                    let lz = l + offset(locations[2])*(l-lf)/0.05;
                    if (0.03..=0.35).contains(&(hz-lz)) {
                        return Some((GtaVec::new(cut.x,cut.y,hz),GtaVec::new(cut.x,cut.y,lz)));
                    }
                }
            }
        }
        let middle = GtaVec::new(p.x, p.y, z);
        if (a.z - z).abs() >= (b.z - z).abs() { b = middle; }
        else { a = middle; }
    }
    if (a.z - b.z).abs() < 0.03 { return None; }
    let (high, mut low) = if a.z > b.z { (a, b) } else { (b, a) };
    low.x = high.x; low.y = high.y;
    Some((high, low))
}

fn step_surfaces(probe: &mut dyn GroundProbe, quad: [GtaVec; 4]) -> Option<Vec<Vec<GtaVec>>> {
    let cuts: [Option<(GtaVec, GtaVec)>; 4] = std::array::from_fn(|i|
        step_between(probe, quad[i], quad[(i + 1) % 4]));
    let crossing: Vec<_> = (0..4).filter(|&i| cuts[i].is_some()).collect();
    if crossing.len() != 2 { return None; }
    let first = crossing[0];
    let mut raised = [false; 4];
    raised[first] = quad[first].z > quad[(first + 1) % 4].z;
    for offset in 0..3 {
        let i = (first + offset) % 4;
        raised[(i + 1) % 4] = if cuts[i].is_some() { !raised[i] } else { raised[i] };
    }
    let mut out = Vec::new();
    for side in [false, true] {
        let mut polygon = Vec::new();
        for i in 0..4 {
            if raised[i] == side { polygon.push(quad[i]); }
            if let Some((high, low)) = cuts[i] { polygon.push(if side { high } else { low }); }
        }
        out.push(polygon);
    }
    let entering = crossing.iter().copied().find(|&i| !raised[i])?;
    let leaving = crossing.iter().copied().find(|&i| raised[i])?;
    let (a, al) = cuts[entering]?;
    let (b, bl) = cuts[leaving]?;
    out.push(vec![a, b, bl, al]);
    Some(out)
}
fn build_surface(
    probe: &mut dyn GroundProbe,
    samples: &[Option<GtaVec>],
    faces: Vec<[usize; 3]>,
    partials: Vec<[usize; 3]>,
    frame: &Frame,
    material: RetailContactMaterial,
) -> Vec<WorldTriangle> {
    let mut points = Vec::new();
    let mut ids = std::collections::HashMap::new();
    let mut triangles = Vec::new();
    for pair in faces.chunks_exact(2) {
        let quad = [pair[0][0], pair[0][1], pair[0][2], pair[1][2]].map(|k| samples[k].unwrap());
        let polygons = step_surfaces(probe, quad).unwrap_or_else(|| vec![quad.to_vec()]);
        for polygon in polygons {
            let indices: Vec<usize> = polygon.iter().map(|&p| {
                let key = [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()];
                *ids.entry(key).or_insert_with(|| {
                    let id = points.len();
                    points.push(frame.to_skate(p));
                    id
                })
            }).collect();
            for i in 1..indices.len() - 1 {
                triangles.push([indices[0], indices[i], indices[i + 1]]);
            }
        }
    }
    for face in partials {
        let indices = face.map(|k| {
            let p=samples[k].unwrap();
            *ids.entry([p.x.to_bits(),p.y.to_bits(),p.z.to_bits()]).or_insert_with(|| {
                let id=points.len();points.push(frame.to_skate(p));id
            })
        });
        triangles.push(indices);
    }
    mesh_triangles(&points, &triangles, material)
}

fn normal([a, b, c]: [Vector3; 3]) -> Option<Vector3> {
    let e1 = Vector3::new(b.x - a.x, b.y - a.y, b.z - a.z);
    let e2 = Vector3::new(c.x - a.x, c.y - a.y, c.z - a.z);
    let n = Vector3::new(e1.y * e2.z - e1.z * e2.y, e1.z * e2.x - e1.x * e2.z, e1.x * e2.y - e1.y * e2.x);
    let len = (n.x * n.x + n.y * n.y + n.z * n.z).sqrt();
    (len > 1e-9).then(|| Vector3::new(n.x / len, n.y / len, n.z / len))
}

/// Triangles of any orientation (normal = (b - a) x (c - a), facing out of
/// a solid), with shared-edge cosines and disabled coplanar internal vertices.
pub fn mesh_triangles(points: &[Vector3], faces: &[[usize; 3]], material: RetailContactMaterial) -> Vec<WorldTriangle> {
    let point = |k: usize| points[k];
    let mut ids = Vec::with_capacity(faces.len());
    let mut normals = Vec::with_capacity(faces.len());
    for &face in faces {
        if let Some(n) = normal(face.map(point)) {
            ids.push(face);
            normals.push(n);
        }
    }
    let dot = |a: Vector3, b: Vector3| a.x * b.x + a.y * b.y + a.z * b.z;
    let cross = |a: Vector3, b: Vector3| {
        Vector3::new(a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x)
    };
    let mut cosines = vec![[1.0f32; 3]; ids.len()];
    let mut flags = vec![TriangleFeature::ONE_SIDED | TriangleFeature::USE_EDGE_COSINES; ids.len()];
    let mut open = std::collections::HashMap::<(usize, usize), (usize, usize)>::new();
    for (i, face) in ids.iter().enumerate() {
        for edge in 0..3 {
            let (a, b) = (face[edge], face[(edge + 1) % 3]);
            if let Some((other, oe)) = open.remove(&(b, a)) {
                let cosine = dot(normals[i], normals[other]).clamp(-1.0, 1.0);
                let (pa, pb) = (point(a), point(b));
                let along = Vector3::new(pb.x - pa.x, pb.y - pa.y, pb.z - pa.z);
                let convex = dot(along, cross(normals[i], normals[other])) > -1.0e-6 && cosine < 1.0;
                for (t, e) in [(i, edge), (other, oe)] {
                    cosines[t][e] = cosine;
                    if convex {
                        flags[t] |= 0x20 << e;
                    }
                }
            } else {
                open.insert((a, b), (i, edge));
            }
        }
    }
    let mut around = std::collections::HashMap::<usize, Vec<usize>>::new();
    for (i, face) in ids.iter().enumerate() {
        for &v in face {
            around.entry(v).or_default().push(i);
        }
    }
    for (v, faces) in &around {
        let reference = normals[faces[0]];
        if faces.iter().all(|&i| (dot(reference, normals[i]) - 1.0).abs() <= 0.01) {
            for &i in faces {
                for corner in 0..3 {
                    if ids[i][corner] == *v {
                        flags[i] |= 0x200 << corner;
                    }
                }
            }
        }
    }
    ids.iter()
        .enumerate()
        .filter_map(|(i, face)| {
            WorldTriangle::from_vertices(face.map(point), material, 0, flags[i], cosines[i], 0.0)
        })
        .collect()
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
pub fn upward_vertices(v: [Vector3; 3]) -> [Vector3; 3] {
    let e1 = Vector3::new(v[1].x - v[0].x, v[1].y - v[0].y, v[1].z - v[0].z);
    let e2 = Vector3::new(v[2].x - v[0].x, v[2].y - v[0].y, v[2].z - v[0].z);
    let ny = e1.z * e2.x - e1.x * e2.z;
    if ny >= 0.0 { v } else { [v[0], v[2], v[1]] }
}


/// Static ground mesh with per-triangle bounds, as the original host builds
/// its levels, so contact queries only visit triangles near each volume.
pub fn world(patch: &Patch) -> BoardWorld {
    world_of(std::slice::from_ref(patch), &[])
}

pub fn world_of(patches: &[Patch], extra: &[WorldTriangle]) -> BoardWorld {
    let triangles: Vec<_> = patches.iter().flat_map(|p| p.triangles.iter().copied()).chain(extra.iter().copied()).collect();
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

    /// GTA XY squares the near patches cover, for the far field to skip.
    pub fn rects(&self) -> Vec<crate::far::Rect> {
        let h = self.half_extent();
        self.patches.iter().map(|p| [p.center.x - h, p.center.y - h, p.center.x + h, p.center.y + h]).collect()
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
        // The refined top, bottom and vertical face add triangles only at the
        // step; the surrounding planar blocks still beat a full grid.
        assert!(patch.triangles.len() > 2 && patch.triangles.len() < full, "{}", patch.triangles.len());
        let steep = patch.triangles.iter().filter(|t| t.triangle.feature.normal.y < 0.99).count();
        assert!(steep > 0, "the curb face is represented");
        for t in &patch.triangles {
            let n = t.triangle.feature.normal;
            assert!(n.y > 0.999 || n.y.abs() < 0.001, "step became a ramp: {n:?}");
            if n.y.abs() < 0.001 {
                assert!(n.x < -0.999, "curb face must face the low side: {n:?}");
                for p in t.triangle.vertices {
                    assert!((p.x - 0.3).abs() < 0.002, "face moved with the grid: {p:?}");
                }
            }
        }
    }

    struct InclinedCurb { bevel: bool }
    impl GroundProbe for InclinedCurb {
        fn down(&mut self,x:f32,y:f32,top:f32,bottom:f32) -> Option<f32> {
            let step = if self.bevel { ((x-0.3)/0.05).clamp(0.,1.)*0.15 }
                else if x>=0.3 {0.15} else {0.};
            let z = 0.05*x + 0.08*y + step;
            (top>=z && bottom<=z).then_some(z)
        }
    }
    #[test]
    fn inclined_and_bevelled_curbs_have_a_face_instead_of_an_entry_ramp() {
        for bevel in [false,true] {
            let patch = sample(&mut InclinedCurb{bevel},&Frame::new(GtaVec::default()),
                GtaVec::default(),&PatchSettings::default(),MATERIAL);
            let mut sides = 0;
            for t in &patch.triangles {
                let n = t.triangle.feature.normal;
                assert!(n.y.abs()<0.001 || n.y>0.99, "artificial curb ramp, bevel {bevel}: {n:?}");
                if n.y.abs()<0.001 { sides+=1; }
            }
            assert!(sides>0,"missing curb face, bevel {bevel}");
        }
    }

    #[test]
    fn a_continuous_steep_transition_is_not_a_curb() {
        struct Ramp;
        impl GroundProbe for Ramp {
            fn down(&mut self,x:f32,_y:f32,_top:f32,_bottom:f32) -> Option<f32> { Some(3.*x) }
        }
        assert!(step_between(&mut Ramp,GtaVec::new(0.,0.,0.),GtaVec::new(0.1,0.,0.3)).is_none());
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
