//! Solid things the downward ground rays cannot describe: walls and
//! building sides (found with sideways rays), and GTA entities (vehicles,
//! pedestrians) as oriented boxes. Static props use native probes. These become closed boxes
//! of world triangles next to the sampled ground, so the original Skate 3
//! physics collides with them, bails included.
use crate::{
    coords::{Frame, GtaVec},
    terrain::{mesh_triangles, GroundProbe},
};
use skate_core::physics::{board_world::WorldTriangle, contact::RetailContactMaterial};
use std::collections::HashMap;

/// An oriented box in GTA space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Obstacle {
    pub center: GtaVec,
    /// Unit axes (right, forward, up for entities).
    pub axes: [GtaVec; 3],
    pub half: [f32; 3],
}

impl Obstacle {
    /// Axis-aligned upright box.
    pub fn upright(center: GtaVec, half: [f32; 3]) -> Self {
        Self { center, axes: [GtaVec::new(1.0, 0.0, 0.0), GtaVec::new(0.0, 1.0, 0.0), GtaVec::new(0.0, 0.0, 1.0)], half }
    }

    /// A slab behind a wall surface: `point` on the face, `normal` horizontal
    /// and pointing out of the wall, spanning heights `bottom..top`.
    pub fn wall(point: GtaVec, normal: GtaVec, bottom: f32, top: f32, width: f32) -> Self {
        let depth = 0.3;
        let n = GtaVec::new(normal.x, normal.y, 0.0).normalized();
        let along = GtaVec::new(-n.y, n.x, 0.0);
        let center = GtaVec::new(point.x - n.x * depth * 0.5, point.y - n.y * depth * 0.5, (bottom + top) * 0.5);
        Self { center, axes: [along, n.scale(-1.0), GtaVec::new(0.0, 0.0, 1.0)], half: [width * 0.5, depth * 0.5, (top - bottom) * 0.5] }
    }

    pub fn corners(&self) -> [GtaVec; 8] {
        std::array::from_fn(|i| {
            let s = |bit: usize| if i >> bit & 1 == 1 { 1.0 } else { -1.0 };
            self.center
                .add(self.axes[0].scale(s(0) * self.half[0]))
                .add(self.axes[1].scale(s(1) * self.half[1]))
                .add(self.axes[2].scale(s(2) * self.half[2]))
        })
    }

    /// Distance from `p` to the box (0 inside).
    pub fn distance(&self, p: GtaVec) -> f32 {
        let d = p.sub(self.center);
        let mut sum = 0.0;
        for k in 0..3 {
            let a = self.axes[k];
            let along = (d.x * a.x + d.y * a.y + d.z * a.z).abs() - self.half[k];
            if along > 0.0 {
                sum += along * along;
            }
        }
        sum.sqrt()
    }
}

/// Box faces wound outward for corner index bits (x: 1, y: 2, z: 4) on a
/// right-handed axis set.
const BOX_FACES: [[usize; 3]; 12] = [
    [0, 2, 3], [0, 3, 1], // -z
    [4, 5, 7], [4, 7, 6], // +z
    [0, 1, 5], [0, 5, 4], // -y
    [2, 6, 7], [2, 7, 3], // +y
    [0, 4, 6], [0, 6, 2], // -x
    [1, 3, 7], [1, 7, 5], // +x
];

/// Side-ray slabs must not duplicate a ledge/tube already represented by
/// the terrain or rail mesh. Their independent end caps interrupt grinding.
pub fn wall_overlaps_grind(wall: &Obstacle, lines: &[Vec<GtaVec>]) -> bool {
    let face=wall.center.sub(wall.axes[1].scale(wall.half[1]));
    let top=wall.center.z+wall.half[2];
    lines.iter().any(|line|line.windows(2).any(|pair| {
        let a=pair[0]; let d=pair[1].sub(a);
        let length2=d.x*d.x+d.y*d.y;
        if length2<1e-6{return false;}
        let along=(d.x*wall.axes[0].x+d.y*wall.axes[0].y).abs()/length2.sqrt();
        if along<0.9{return false;}
        let t=(((face.x-a.x)*d.x+(face.y-a.y)*d.y)/length2).clamp(0.,1.);
        let at=a.add(d.scale(t));
        (at.x-face.x).powi(2)+(at.y-face.y).powi(2)<0.08*0.08 && (at.z-top).abs()<0.12
    }))
}

/// Closed boxes as skate-space world triangles.
pub fn triangles(obstacles: &[Obstacle], frame: &Frame, material: RetailContactMaterial) -> Vec<WorldTriangle> {
    let mut out = Vec::new();
    for o in obstacles {
        if o.half.iter().any(|h| !(h.is_finite() && *h > 0.005)) {
            continue;
        }
        let corners = o.corners();
        let points: Vec<_> = corners.iter().map(|&c| frame.to_skate(c)).collect();
        // A left-handed axis set mirrors the box; flip windings to keep them outward.
        let a = o.axes;
        let cross = GtaVec::new(a[0].y * a[1].z - a[0].z * a[1].y, a[0].z * a[1].x - a[0].x * a[1].z, a[0].x * a[1].y - a[0].y * a[1].x);
        let right_handed = cross.x * a[2].x + cross.y * a[2].y + cross.z * a[2].z > 0.0;
        let faces: Vec<[usize; 3]> =
            BOX_FACES.iter().map(|&[i, j, k]| if right_handed { [i, j, k] } else { [i, k, j] }).collect();
        out.extend(mesh_triangles(&points, &faces, material));
    }
    out
}

#[derive(Clone, Copy, Debug)]
pub struct WallSettings {
    /// Sideways ray length, metres.
    pub reach: f32,
    /// Ray heights above the ground under the skater.
    pub heights: [f32; 2],
    /// Directions around the skater; `rays` of them are cast per update.
    pub directions: usize,
    pub rays: usize,
    /// Tallest wall described (the rest of a building is out of reach anyway).
    pub tallest: f32,
    /// Updates a wall survives without being seen again.
    pub keep_updates: u32,
}

impl Default for WallSettings {
    fn default() -> Self {
        Self { reach: 3.0, heights: [0.35, 1.0], directions: 16, rays: 8, tallest: 3.5, keep_updates: 30 }
    }
}

struct Wall {
    obstacle: Obstacle,
    seen: u32,
}

/// Walls around the skater from sideways rays, kept for a while once seen.
pub struct WallFinder {
    pub settings: WallSettings,
    walls: HashMap<(i32, i32, i32), Wall>,
    next: usize,
    updates: u32,
}

impl WallFinder {
    pub fn new(settings: WallSettings) -> Self {
        Self { settings, walls: HashMap::new(), next: 0, updates: 0 }
    }

    /// `ground` is the point on the ground under the skater.
    pub fn update(&mut self, probe: &mut dyn GroundProbe, ground: GtaVec) {
        let s = self.settings;
        self.updates += 1;
        let casts = s.directions * s.heights.len();
        for _ in 0..s.rays.min(casts) {
            let k = self.next % casts;
            self.next += 1;
            let angle = (k % s.directions) as f32 / s.directions as f32 * std::f32::consts::TAU;
            let height = s.heights[k / s.directions];
            let from = ground.add(GtaVec::new(0.0, 0.0, height));
            let to = from.add(GtaVec::new(angle.cos() * s.reach, angle.sin() * s.reach, 0.0));
            let Some((hit, normal)) = probe.toward(from, to) else { continue };
            // Slopes and ramps face up; only near-vertical faces are walls.
            if normal.z.abs() > 0.10 {
                continue;
            }
            let n = GtaVec::new(normal.x, normal.y, 0.0).normalized();
            // The almost vertical end of a quarter-pipe also faces sideways.
            // If the sampled ground continues through it, terrain already
            // describes this face; adding a slab would block the transition.
            let front=hit.add(n.scale(0.15));
            let back=hit.sub(n.scale(0.15));
            if let (Some(a),Some(b))=(
                probe.down(front.x,front.y,hit.z+4.,hit.z-4.),
                probe.down(back.x,back.y,hit.z+4.,hit.z-4.)) {
                if (a-b).abs()>0.03 && crate::terrain::step_between(probe,
                    GtaVec::new(front.x,front.y,a),GtaVec::new(back.x,back.y,b)).is_none() {
                    continue;
                }
            }
            // Its top: the first surface found just inside the face, from above.
            let inside = hit.sub(n.scale(0.01));
            let ceiling = ground.z + s.tallest;
            let found_top = probe
                .down(inside.x, inside.y, ceiling, ground.z - 0.5)
                .filter(|&t| t > ground.z + 0.05 && t < ceiling - 0.05);
            let top = found_top.unwrap_or_else(|| {
                // A missed top ray is not evidence for a tall wall. Confirm
                // the same face upward, then refine its actual end.
                let same_face=|probe:&mut dyn GroundProbe,z:f32| {
                    let at=GtaVec::new(hit.x,hit.y,z);
                    probe.toward(at.add(n.scale(0.15)),at.sub(n.scale(0.15)))
                        .is_some_and(|(p,facing)| facing.z.abs()<=0.10
                            && facing.x*n.x+facing.y*n.y>=0.98
                            && ((p.x-hit.x)*n.x+(p.y-hit.y)*n.y).abs()<=0.02)
                };
                let mut confirmed=hit.z;
                while confirmed<ceiling {
                    let next=(confirmed+0.2).min(ceiling);
                    if same_face(probe,next){confirmed=next;continue;}
                    let mut end=next;
                    for _ in 0..5 {
                        let middle=(confirmed+end)*0.5;
                        if same_face(probe,middle){confirmed=middle;}else{end=middle;}
                    }
                    break;
                }
                confirmed
            });
            // Do not extend a tiny pole into a metre-wide slab. Each tangent
            // offset must hit the same plane with the same facing normal.
            let along = GtaVec::new(-n.y, n.x, 0.0);
            let mut extents = [0.025f32; 2];
            for (side, sign) in [-1.0, 1.0].into_iter().enumerate() {
                for step in 1..=4 {
                    let offset = along.scale(sign * step as f32 * 0.1);
                    let start = hit.add(offset).add(n.scale(0.15));
                    let end = hit.add(offset).sub(n.scale(0.15));
                    let Some((p, facing)) = probe.toward(start, end) else { break };
                    let d = p.sub(hit);
                    let plane_error = (d.x * n.x + d.y * n.y).abs();
                    if facing.z.abs() > 0.10 || facing.x * n.x + facing.y * n.y < 0.98 || plane_error > 0.02 { break; }
                    extents[side] = step as f32 * 0.1;
                }
            }
            // A horizontal handrail is not a wall down to the pavement.
            // Only extend downward while the same vertical face is observed.
            let mut bottom = hit.z;
            let steps = ((hit.z-(ground.z-0.5))/0.2).ceil().max(0.) as usize;
            for step in 1..=steps.min(12) {
                let at = GtaVec::new(hit.x,hit.y,hit.z-step as f32*0.2);
                let Some((p,facing)) = probe.toward(at.add(n.scale(0.15)),at.sub(n.scale(0.15))) else {break};
                let d=p.sub(at);
                if facing.z.abs()>0.10 || facing.x*n.x+facing.y*n.y<0.98
                    || (d.x*n.x+d.y*n.y).abs()>0.02 {break;}
                bottom=p.z;
            }
            let mid = hit.add(along.scale((extents[1] - extents[0]) * 0.5));
            let obstacle = Obstacle::wall(mid, n, bottom-0.01, top.max(hit.z+0.01), extents[0] + extents[1]);
            let key = ((hit.x * 4.0).round() as i32, (hit.y * 4.0).round() as i32, (top * 4.0).round() as i32);
            self.walls.insert(key, Wall { obstacle, seen: self.updates });
        }
        let (now, keep) = (self.updates, s.keep_updates);
        let reach = s.reach + 2.0;
        self.walls.retain(|_, w| {
            let d = w.obstacle.center.sub(ground);
            now - w.seen <= keep && (d.x * d.x + d.y * d.y).sqrt() <= reach
        });
    }

    pub fn obstacles(&self) -> Vec<Obstacle> {
        let mut keys: Vec<_> = self.walls.keys().copied().collect();
        keys.sort_unstable();
        keys.iter().map(|k| self.walls[k].obstacle).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grind_slab_duplicates_are_removed_but_crossing_walls_remain() {
        let wall=Obstacle::wall(GtaVec::new(1.,0.,0.3),GtaVec::new(-1.,0.,0.),0.,0.5,0.8);
        let line=vec![GtaVec::new(1.,-2.,0.5),GtaVec::new(1.,2.,0.5)];
        assert!(wall_overlaps_grind(&wall,&[line]));
        let crossing=vec![GtaVec::new(-1.,0.,0.5),GtaVec::new(3.,0.,0.5)];
        assert!(!wall_overlaps_grind(&wall,&[crossing]));
        let remote=vec![GtaVec::new(2.,-2.,0.5),GtaVec::new(2.,2.,0.5)];
        assert!(!wall_overlaps_grind(&wall,&[remote]));
    }

    #[test]
    fn an_almost_vertical_continuous_transition_is_not_boxed() {
        struct Transition;
        impl GroundProbe for Transition {
            fn down(&mut self,x:f32,_y:f32,top:f32,bottom:f32)->Option<f32> {
                let z=20.*(x-1.);
                (top>=z && bottom<=z).then_some(z)
            }
            fn toward(&mut self,from:GtaVec,to:GtaVec)->Option<(GtaVec,GtaVec)> {
                let d=to.sub(from); let denominator=d.z-20.*d.x;
                if denominator.abs()<1e-6{return None;}
                let t=(20.*(from.x-1.)-from.z)/denominator;
                (0. ..=1.).contains(&t).then_some((from.add(d.scale(t)),GtaVec::new(-20.,0.,1.).normalized()))
            }
        }
        let mut finder=WallFinder::new(WallSettings{rays:1000,..WallSettings::default()});
        finder.update(&mut Transition,GtaVec::default());
        assert!(finder.obstacles().is_empty(),"continuous near-vertical ramp acquired slabs");
    }

    #[test]
    fn missing_top_ray_does_not_create_a_tall_phantom_wall() {
        struct Fin;
        impl GroundProbe for Fin {
            fn down(&mut self,_x:f32,_y:f32,top:f32,bottom:f32)->Option<f32> {
                (top>=0. && bottom<=0.).then_some(0.)
            }
            fn toward(&mut self,from:GtaVec,to:GtaVec)->Option<(GtaVec,GtaVec)> {
                let d=to.sub(from);
                if d.x<=0. || !(0. ..=0.5).contains(&from.z){return None;}
                let t=(1.-from.x)/d.x;
                (0. ..=1.).contains(&t).then_some((from.add(d.scale(t)),GtaVec::new(-1.,0.,0.)))
            }
        }
        let mut finder=WallFinder::new(WallSettings{rays:1000,..WallSettings::default()});
        finder.update(&mut Fin,GtaVec::default());
        let obstacles=finder.obstacles();
        assert!(!obstacles.is_empty());
        for o in obstacles {assert!(o.center.z+o.half[2]<=0.51,"invented wall top: {o:?}");}
    }

    #[test]
    fn ramps_are_not_walls_and_a_pole_does_not_fill_the_road() {
        struct Scene { ramp: bool }
        impl GroundProbe for Scene {
            fn down(&mut self,_x:f32,_y:f32,_top:f32,_bottom:f32)->Option<f32>{Some(0.)}
            fn toward(&mut self,from:GtaVec,to:GtaVec)->Option<(GtaVec,GtaVec)>{
                let d=to.sub(from);
                if d.x==0. {return None;}
                let t=(1.-from.x)/d.x;
                if !(0. ..=1.).contains(&t){return None;}
                let p=from.add(d.scale(t));
                if self.ramp {Some((p,GtaVec::new(-0.92,0.,0.39)))}
                else {(p.y.abs()<0.04).then_some((p,GtaVec::new(-1.,0.,0.)))}
            }
        }
        let settings=WallSettings{rays:1000,..WallSettings::default()};
        let mut finder=WallFinder::new(settings);
        finder.update(&mut Scene{ramp:true},GtaVec::default());
        assert!(finder.obstacles().is_empty(),"ramp was boxed as a building wall");
        finder.update(&mut Scene{ramp:false},GtaVec::default());
        let obstacles=finder.obstacles();
        assert!(!obstacles.is_empty(),"the pole must remain solid");
        for o in obstacles {assert!(o.half[0]*2.<0.1,"pole expanded into the road: {o:?}");}
    }

    #[test]
    fn box_faces_point_outward() {
        let frame = Frame::facing(GtaVec::new(10.0, 20.0, 30.0), 37.0);
        let material = RetailContactMaterial { static_friction: 0.5, dynamic_friction: 0.5, restitution: 0.0 };
        let o = Obstacle::upright(GtaVec::new(12.0, 21.0, 31.0), [1.0, 0.5, 0.75]);
        let tris = triangles(&[o], &frame, material);
        assert_eq!(tris.len(), 12);
        let center = frame.to_skate(o.center);
        for t in &tris {
            let [a, b, c] = t.triangle.vertices;
            let e1 = [b.x - a.x, b.y - a.y, b.z - a.z];
            let e2 = [c.x - a.x, c.y - a.y, c.z - a.z];
            let n = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
            let to_face = [a.x - center.x, a.y - center.y, a.z - center.z];
            assert!(n[0] * to_face[0] + n[1] * to_face[1] + n[2] * to_face[2] > 0.0, "outward");
        }
        assert!(o.distance(GtaVec::new(12.0, 21.0, 31.0)) == 0.0);
        assert!((o.distance(GtaVec::new(14.0, 21.0, 31.0)) - 1.0).abs() < 1e-5);
    }

    /// A building wall facing west at x = 5, 10 m tall, and a 40 cm ledge
    /// facing south at y = 4.
    struct Block;
    impl GroundProbe for Block {
        fn down(&mut self, x: f32, y: f32, top: f32, bottom: f32) -> Option<f32> {
            let z = if x >= 5.0 { 10.0 } else if y >= 4.0 { 0.4 } else { 0.0 };
            // Rays starting inside the building see nothing, as in GTA.
            if x >= 5.0 && top < 10.0 {
                return None;
            }
            (top >= z && bottom <= z).then_some(z)
        }
        fn toward(&mut self, from: GtaVec, to: GtaVec) -> Option<(GtaVec, GtaVec)> {
            let d = to.sub(from);
            let mut best: Option<(f32, GtaVec)> = None;
            if d.x > 0.0 {
                let t = (5.0 - from.x) / d.x;
                if (0.0..=1.0).contains(&t) {
                    best = Some((t, GtaVec::new(-1.0, 0.0, 0.0)));
                }
            }
            if d.y > 0.0 && from.z < 0.4 {
                let t = (4.0 - from.y) / d.y;
                if (0.0..=1.0).contains(&t) && best.is_none_or(|b| t < b.0) && from.x + d.x * t < 5.0 {
                    best = Some((t, GtaVec::new(0.0, -1.0, 0.0)));
                }
            }
            best.map(|(t, n)| (from.add(d.scale(t)), n))
        }
    }

    #[test]
    fn walls_and_ledge_sides_are_found_with_their_heights() {
        let mut finder = WallFinder::new(WallSettings { rays: 1000, ..WallSettings::default() });
        finder.update(&mut Block, GtaVec::new(3.0, 2.0, 0.0));
        let walls = finder.obstacles();
        let building: Vec<_> = walls.iter().filter(|w| w.axes[1].x > 0.9).collect();
        let ledge: Vec<_> = walls.iter().filter(|w| w.axes[1].y > 0.9).collect();
        assert!(!building.is_empty() && !ledge.is_empty(), "{walls:?}");
        for w in &building {
            let top = w.center.z + w.half[2];
            assert!((top - 3.5).abs() < 1e-4, "a tall wall is cut at the reach height: {w:?}");
            assert!((w.center.x - w.half[1] - 5.0).abs() < 1e-4, "face on the wall: {w:?}");
        }
        for w in &ledge {
            let top = w.center.z + w.half[2];
            assert!((top - 0.4).abs() < 1e-4, "a ledge side ends at the ledge top: {w:?}");
        }
    }
}
