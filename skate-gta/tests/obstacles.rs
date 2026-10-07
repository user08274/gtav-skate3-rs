//! Walls and entity boxes with the player's converted files
//! (SKATE_GTA_ASSETS); skipped otherwise.
use skate_core::player::state::PhysicalStateId;
use skate_gameplay::host::Mode;
use skate_gta::{
    coords::GtaVec,
    obstacles::Obstacle,
    ride::Ride,
    terrain::{GroundProbe, PatchSettings},
};
use std::time::Duration;

const STREET: f32 = 31.5;
const WALL_Y: f32 = 209.0;

/// A building face across the street north of the spawn. Like GTA, rays
/// that start inside it see nothing, so the ground grid shows no wall.
struct Building;
impl GroundProbe for Building {
    fn down(&mut self, _x: f32, y: f32, top: f32, bottom: f32) -> Option<f32> {
        if y >= WALL_Y {
            return (top >= STREET + 20.0).then_some(STREET + 20.0);
        }
        (top >= STREET && bottom <= STREET).then_some(STREET)
    }
    fn toward(&mut self, from: GtaVec, to: GtaVec) -> Option<(GtaVec, GtaVec)> {
        let d = to.sub(from);
        if d.y <= 0.0 || from.y >= WALL_Y {
            return None;
        }
        let t = (WALL_Y - from.y) / d.y;
        (0.0..=1.0).contains(&t).then(|| (from.add(d.scale(t)), GtaVec::new(0.0, -1.0, 0.0)))
    }
}

struct Street;
impl GroundProbe for Street {
    fn down(&mut self, _x: f32, _y: f32, top: f32, bottom: f32) -> Option<f32> {
        (top >= STREET && bottom <= STREET).then_some(STREET)
    }
}

fn frame() -> Duration {
    Duration::from_secs_f32(1.0 / 60.0)
}

fn ride_north(probe: &mut dyn GroundProbe, walls: bool, entities: Vec<Obstacle>) -> (f32, Vec<PhysicalStateId>) {
    let root = std::env::var_os("SKATE_GTA_ASSETS").unwrap();
    let mut ride = Ride::start(
        std::path::Path::new(&root),
        Mode::Easy,
        GtaVec::new(100.0, 200.0, STREET),
        0.0,
        PatchSettings::default(),
        probe,
    )
    .unwrap();
    ride.find_walls = walls;
    ride.set_entities(entities);
    let (mut furthest, mut states) = (0.0f32, Vec::new());
    for i in 0..420u32 {
        let mut pad = [0.0; 18];
        pad[16] = if i < 300 && i % 60 < 30 { 1.0 } else { 0.0 };
        ride.advance(frame(), 4, pad, probe).unwrap();
        furthest = furthest.max(ride.deck_position().y).max(ride.hips_position().y);
        let state = ride.game.state();
        if states.last() != Some(&state) {
            states.push(state);
        }
    }
    (furthest, states)
}

fn on_big_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new().stack_size(1 << 20).spawn(f).unwrap().join().unwrap();
}

#[test]
fn a_building_wall_stops_the_skater() {
    if std::env::var_os("SKATE_GTA_ASSETS").is_none() {
        return;
    }
    on_big_stack(|| {
        let (without, _) = ride_north(&mut Building, false, Vec::new());
        let (with, states) = ride_north(&mut Building, true, Vec::new());
        eprintln!("furthest north without walls {without:.2}, with walls {with:.2}, states {states:?}");
        assert!(without > WALL_Y + 1.0, "without walls the skater passes through ({without})");
        assert!(with < WALL_Y + 0.3, "the wall stops the skater ({with})");
    });
}

#[test]
fn a_parked_car_box_stops_the_skater() {
    if std::env::var_os("SKATE_GTA_ASSETS").is_none() {
        return;
    }
    on_big_stack(|| {
        // A car parked across the street, 1.5 m tall.
        let car = Obstacle::upright(GtaVec::new(100.0, 210.0, STREET + 0.75), [2.3, 1.0, 0.75]);
        let (with, states) = ride_north(&mut Street, false, vec![car]);
        eprintln!("furthest north with a car at y 209..211: {with:.2}, states {states:?}");
        assert!(with < 209.3, "the car stops the skater ({with})");
    });
}

/// A car driving past rebuilds the world every frame; it must stay cheap.
#[test]
fn a_moving_car_keeps_ticks_cheap() {
    if std::env::var_os("SKATE_GTA_ASSETS").is_none() {
        return;
    }
    on_big_stack(|| {
        let root = std::env::var_os("SKATE_GTA_ASSETS").unwrap();
        let mut probe = Street;
        let mut ride = Ride::start(std::path::Path::new(&root), Mode::Easy, GtaVec::new(100.0, 200.0, STREET), 0.0, PatchSettings::default(), &mut probe).unwrap();
        let mut props: Vec<Obstacle> = (0..40).map(|i| Obstacle::upright(GtaVec::new(90.0 + (i % 8) as f32 * 2.5, 195.0 + (i / 8) as f32 * 3.0, STREET + 0.5), [0.3, 0.3, 0.5])).collect();
        props.retain(|p| p.distance(GtaVec::new(100.0, 200.0, STREET)) > 2.0);
        let start = std::time::Instant::now();
        for i in 0..240u32 {
            let mut boxes = props.clone();
            boxes.push(Obstacle::upright(GtaVec::new(94.0, 180.0 + i as f32 * 0.15, STREET + 0.75), [1.0, 2.3, 0.75]));
            ride.set_entities(boxes);
            ride.advance(frame(), 4, [0.0; 18], &mut probe).unwrap();
        }
        let per = start.elapsed() / 240;
        eprintln!("frame with a moving car and {} props: {per:?}", props.len());
        assert!(per < std::time::Duration::from_millis(8), "{per:?}");
    });
}
