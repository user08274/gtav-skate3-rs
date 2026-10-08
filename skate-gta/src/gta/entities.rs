//! GTA entities around the skater as Skate 3 obstacles: vehicles
//! as boxes from their model bounds, pedestrians as upright boxes. Static props
//! use native collision probes rather than their entire model bounds. Peds the
//! skater runs into are knocked over (GTA ragdoll), as in Skate 3.
use super::{natives as n, shv};
use crate::{coords::GtaVec, obstacles::Obstacle};
use std::collections::HashMap;

const VEHICLE_RADIUS: f32 = 12.0;
const PED_RADIUS: f32 = 6.0;
/// A ped this close to the skater at this speed is knocked over.
const KNOCK_DISTANCE: f32 = 0.35;
const KNOCK_SPEED: f32 = 2.0;
const PED_HALF: [f32; 3] = [0.25, 0.25, 0.9];

#[derive(Default)]
pub struct Entities {
    frames: u32,
    /// Peds knocked over recently: frame of the knock.
    knocked: HashMap<i32, u32>,
    pub knocked_total: u32,
}

fn horizontal(a: GtaVec, b: GtaVec) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}

fn model_box(entity: i32) -> Option<Obstacle> {
    let (min, max) = n::get_model_dimensions(n::get_entity_model(entity));
    let [right, forward, up, position] = n::get_entity_matrix(entity);
    let half = [(max.x - min.x) * 0.5, (max.y - min.y) * 0.5, (max.z - min.z) * 0.5];
    if half.iter().any(|h| !(h.is_finite() && *h > 0.0)) {
        return None;
    }
    let local = min.add(max).scale(0.5);
    let center = position.add(right.scale(local.x)).add(forward.scale(local.y)).add(up.scale(local.z));
    Some(Obstacle { center, axes: [right.normalized(), forward.normalized(), up.normalized()], half })
}

impl Entities {
    /// Boxes for everything solid near `center`, and the peds among them.
    pub fn gather(&mut self, player: i32, _board: Option<i32>, center: GtaVec) -> (Vec<Obstacle>, Vec<(i32, Obstacle)>) {
        self.frames += 1;
        let mut boxes = Vec::new();
        for v in shv::world_entities(shv::Pool::Vehicles) {
            if horizontal(n::get_entity_coords(v), center) > VEHICLE_RADIUS || !n::is_entity_visible(v) {
                continue;
            }
            boxes.extend(model_box(v));
        }
        let mut peds = Vec::new();
        for p in shv::world_entities(shv::Pool::Peds) {
            if p == player {
                continue;
            }
            let at = n::get_entity_coords(p);
            if horizontal(at, center) > PED_RADIUS
                || n::is_entity_dead(p)
                || n::is_ped_ragdoll(p)
                || n::is_ped_in_any_vehicle(p)
                || !n::is_entity_visible(p)
            {
                continue;
            }
            let b = Obstacle::upright(at.sub(GtaVec::new(0.0, 0.0, 0.1)), PED_HALF);
            boxes.push(b);
            peds.push((p, b));
        }
        // Static props use GTA's actual collision probes. A model bounding box
        // fills the air below a traffic-light arm and inside an entire fence.
        (boxes, peds)
    }

    /// Knocks over peds the skater touches while moving; returns how many.
    pub fn knock(&mut self, peds: &[(i32, Obstacle)], skater: &[GtaVec], velocity: GtaVec) -> u32 {
        let speed = (velocity.x * velocity.x + velocity.y * velocity.y).sqrt();
        if speed < KNOCK_SPEED {
            return 0;
        }
        let frames = self.frames;
        self.knocked.retain(|_, at| frames - *at < 180);
        let mut count = 0;
        for (ped, b) in peds {
            if self.knocked.contains_key(ped) || !skater.iter().any(|&p| b.distance(p) < KNOCK_DISTANCE) {
                continue;
            }
            n::set_ped_to_ragdoll(*ped, 2000);
            let push = (speed * 8.0).min(60.0);
            let dir = GtaVec::new(velocity.x / speed, velocity.y / speed, 0.0);
            n::push_entity(*ped, GtaVec::new(dir.x * push, dir.y * push, push * 0.2));
            self.knocked.insert(*ped, frames);
            count += 1;
        }
        self.knocked_total += count;
        count
    }
}
