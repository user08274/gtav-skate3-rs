//! Coarse GTA height field around the skater for line queries only. The
//! Skate 3 camera (collision, drop and trajectory prediction) and ground
//! probes cast lines metres away from the board; contacts keep using the
//! fine near patches, which this field defers to wherever they exist.
use crate::{
    coords::{Frame, GtaVec},
    terrain::{GroundProbe, upward_vertices},
};
use skate_core::{
    math::Vector3,
    physics::{
        board_world::{ExternalLineHit, ExternalQueries, WorldLineHit},
        triangle_query::{TriangleLineHit, triangle_segment},
    },
};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
pub struct FarSettings {
    pub spacing: f32,
    pub radius_cells: i32,
    /// Synchronous GTA probes allowed per frame; the field fills in over frames.
    pub probe_budget: usize,
    pub probe_above: f32,
    pub probe_below: f32,
}

impl Default for FarSettings {
    fn default() -> Self {
        Self { spacing: 1.0, radius_cells: 16, probe_budget: 120, probe_above: 6.0, probe_below: 10.0 }
    }
}

type Key = (i32, i32);

pub struct FarField {
    pub settings: FarSettings,
    samples: HashMap<Key, Option<f32>>,
}

/// GTA-space XY rectangle covered by a near patch: [min_x, min_y, max_x, max_y].
pub type Rect = [f32; 4];

impl FarField {
    pub fn new(settings: FarSettings) -> Self {
        Self { settings, samples: HashMap::new() }
    }

    fn key(&self, x: f32, y: f32) -> Key {
        ((x / self.settings.spacing).round() as i32, (y / self.settings.spacing).round() as i32)
    }

    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    /// Drops samples that fell behind and probes the nearest missing ones.
    pub fn update(&mut self, probe: &mut dyn GroundProbe, center: GtaVec) {
        let (ci, cj) = self.key(center.x, center.y);
        let r = self.settings.radius_cells;
        self.samples.retain(|&(i, j), _| (i - ci).abs() <= r + 2 && (j - cj).abs() <= r + 2);
        let mut missing: Vec<Key> = (-r..=r)
            .flat_map(|dj| (-r..=r).map(move |di| (ci + di, cj + dj)))
            .filter(|k| !self.samples.contains_key(k))
            .collect();
        missing.sort_by_key(|&(i, j)| (i - ci).pow(2) + (j - cj).pow(2));
        let s = self.settings.spacing;
        for (i, j) in missing.into_iter().take(self.settings.probe_budget) {
            let (x, y) = (i as f32 * s, j as f32 * s);
            let z = probe.down(x, y, center.z + self.settings.probe_above, center.z - self.settings.probe_below);
            self.samples.insert((i, j), z);
        }
    }

    pub fn queries(&self, frame: Frame, near: Vec<Rect>) -> FarQueries {
        FarQueries {
            frame,
            spacing: self.settings.spacing,
            heights: self.samples.iter().filter_map(|(k, z)| z.map(|z| (*k, z))).collect(),
            near,
        }
    }
}

/// Immutable snapshot handed to the gameplay thread.
pub struct FarQueries {
    frame: Frame,
    spacing: f32,
    heights: HashMap<Key, f32>,
    near: Vec<Rect>,
}

const MAX_CELLS_PER_AXIS: i32 = 48;

impl FarQueries {
    fn corner(&self, i: i32, j: i32) -> Option<GtaVec> {
        self.heights.get(&(i, j)).map(|&z| GtaVec::new(i as f32 * self.spacing, j as f32 * self.spacing, z))
    }

    fn covered_by_near(&self, x: f32, y: f32) -> bool {
        self.near.iter().any(|r| x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3])
    }
}

impl ExternalQueries for FarQueries {
    fn line(&self, start: Vector3, end: Vector3, radius: f32) -> Option<ExternalLineHit> {
        let a = self.frame.to_gta(start);
        let b = self.frame.to_gta(end);
        let pad = radius + self.spacing;
        let cell = |v: f32| (v / self.spacing).floor() as i32;
        let (i0, i1) = (cell(a.x.min(b.x) - pad), cell(a.x.max(b.x) + pad));
        let (j0, j1) = (cell(a.y.min(b.y) - pad), cell(a.y.max(b.y) + pad));
        if i1 - i0 > MAX_CELLS_PER_AXIS || j1 - j0 > MAX_CELLS_PER_AXIS {
            return None;
        }
        let direction = Vector3::new(end.x - start.x, end.y - start.y, end.z - start.z);
        let mut best: Option<TriangleLineHit> = None;
        for j in j0..=j1 {
            for i in i0..=i1 {
                let (Some(p00), Some(p10), Some(p11), Some(p01)) =
                    (self.corner(i, j), self.corner(i + 1, j), self.corner(i + 1, j + 1), self.corner(i, j + 1))
                else {
                    continue;
                };
                let mid = (p00.x + p11.x) * 0.5;
                if self.covered_by_near(mid, (p00.y + p11.y) * 0.5) {
                    continue;
                }
                for tri in [[p00, p10, p11], [p00, p11, p01]] {
                    let vertices = upward_vertices(tri.map(|p| self.frame.to_skate(p)));
                    let mut hit = TriangleLineHit {
                        position: Vector3::ZERO,
                        normal: Vector3::ZERO,
                        fraction: 0.0,
                        volume_parameter: [0.0; 3],
                    };
                    if triangle_segment(&mut hit, start, direction, vertices, radius, 0.0)
                        && best.as_ref().is_none_or(|b| hit.fraction < b.fraction)
                    {
                        best = Some(hit);
                    }
                }
            }
        }
        best.map(|geometry| ExternalLineHit {
            hit: WorldLineHit { geometry, tag: 0 },
            surface: 0,
            geometry_id: 0,
            frame: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]],
        })
    }

    fn nearby(&self, _center: Vector3, _radius: f32) -> Vec<[Vector3; 3]> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Plane;
    impl GroundProbe for Plane {
        fn down(&mut self, _x: f32, _y: f32, top: f32, bottom: f32) -> Option<f32> {
            (top >= 10.0 && bottom <= 10.0).then_some(10.0)
        }
    }

    #[test]
    fn fills_in_over_frames_within_the_budget() {
        let mut field = FarField::new(FarSettings::default());
        field.update(&mut Plane, GtaVec::new(0.0, 0.0, 10.0));
        assert_eq!(field.sample_count(), FarSettings::default().probe_budget);
        for _ in 0..20 {
            field.update(&mut Plane, GtaVec::new(0.0, 0.0, 10.0));
        }
        assert_eq!(field.sample_count(), 33 * 33);
    }

    #[test]
    fn lines_hit_the_far_ground_but_not_inside_near_patches() {
        let mut field = FarField::new(FarSettings::default());
        for _ in 0..20 {
            field.update(&mut Plane, GtaVec::new(0.0, 0.0, 10.0));
        }
        let frame = Frame::new(GtaVec::new(0.0, 0.0, 10.0));
        let down = |x: f32, y: f32| (frame.to_skate(GtaVec::new(x, y, 12.0)), frame.to_skate(GtaVec::new(x, y, 8.0)));
        let open = field.queries(frame, Vec::new());
        let (s, e) = down(7.3, -4.2);
        let hit = open.line(s, e, 0.0).expect("far ground is hit");
        assert!((hit.hit.geometry.fraction - 0.5).abs() < 1e-4);
        assert!(hit.hit.geometry.normal.y > 0.99);
        let shadowed = field.queries(frame, vec![[5.0, -6.0, 9.0, -2.0]]);
        assert!(shadowed.line(s, e, 0.0).is_none(), "near patches answer their own area");
    }
}
