//! The board on synthetic GTA ground. The collections fixture is built from
//! skate-core's decoded stock constants plus test-only materials; the mod
//! itself only ever loads the player's converted files.
use skate_core::physics::{
    drive_frames::RETAIL_DEFAULT_TRUCK_TRANSFORM_INPUTS,
    drive_parameters::RetailTruckDriveSettings,
    mass::{DeckGeometrySettings, TruckMassSettings, WheelMassSettings},
};
use skate_data::collections::Collections;
use skate_gta::{
    coords::GtaVec,
    sim::{BoardSim, Controls, TestControlTuning},
    terrain::{GroundProbe, PatchSettings},
};

struct Flat;
impl GroundProbe for Flat {
    fn down(&mut self, _x: f32, _y: f32, top: f32, bottom: f32) -> Option<f32> {
        (top >= 0.0 && bottom <= 0.0).then_some(0.0)
    }
}

fn float(v: f32) -> serde_json::Value {
    serde_json::json!({"type": "EA::Reflection::Float", "data": format!("{:08X}", v.to_bits())})
}
fn boolean(v: bool) -> serde_json::Value {
    serde_json::json!({"type": "EA::Reflection::Bool", "data": if v { "01000000" } else { "00000000" }})
}
fn int(v: i32) -> serde_json::Value {
    serde_json::json!({"type": "EA::Reflection::Int32", "data": format!("{:08X}", v as u32)})
}

fn fixture() -> Collections {
    let wheel = WheelMassSettings::STOCK;
    let truck = TruckMassSettings::STOCK;
    let deck = DeckGeometrySettings::STOCK;
    let drives = RetailTruckDriveSettings::STOCK;
    let trucks = RETAIL_DEFAULT_TRUCK_TRANSFORM_INPUTS;
    let class = |name: &str, fields: serde_json::Value| {
        serde_json::json!({"class": name, "key": "default", "parent": "", "source": "test", "sha256": "", "fields": fields})
    };
    let gravity = [0.0f32, -9.8, 0.0, 0.0].map(|v| format!("{:08X}", v.to_bits())).concat();
    let json = serde_json::json!({"version": 1, "collections": [
        class("physics_world", serde_json::json!({"SkateboardMassFactor": float(wheel.mass_factor)})),
        class("physicswheels", serde_json::json!({
            "WheelRadius": float(wheel.radius), "WheelXDist": float(truck.wheel_x_distance),
            "WheelMass": float(wheel.mass), "WheelStaticFriction": float(0.8),
            "WheelDynamicFriction": float(0.7), "WheelRestitution": float(0.0)})),
        class("physicsdeck", serde_json::json!({
            "DeckMidLength": float(deck.mid_length), "DeckWidth": float(deck.width),
            "DeckThickness": float(deck.thickness), "DeckBackEndSize": float(deck.back_end_size),
            "DeckFrontEndAngle": float(deck.front_end_angle_degrees),
            "DeckBackEndAngle": float(deck.back_end_angle_degrees),
            "DeckEndCapsules": int(deck.end_capsule_count),
            "DeckEnableDeckVolumeCollisions": boolean(deck.enable_deck_volume_collisions),
            "DeckEnableEndVolumeCollisions": boolean(deck.enable_end_volume_collisions),
            "DeckMass": float(1.2), "DeckAngularDrag": float(f32::from_bits(0x3EE6_6666)),
            "UseHardDrives": boolean(drives.use_hard_linear_drives), "DeckForceYOffset": float(0.0),
            "DeckStaticFriction": float(0.5), "DeckDynamicFriction": float(0.4), "DeckRestitution": float(0.0)})),
        class("physicstrucks", serde_json::json!({
            "TruckZPosFront": float(trucks.truck_z_position_front),
            "TruckZPosBack": float(trucks.truck_z_position_back),
            "TruckYPos": float(trucks.truck_y_position), "RadiusScalar": float(truck.radius_scalar),
            "HalfHeightScalar": float(truck.half_height_scalar), "TruckMass": float(truck.mass),
            "TruckRotationAxisAngle": float(trucks.truck_rotation_axis_angle_degrees),
            "UseTruckDrives": boolean(drives.use_linear_drives),
            "TruckEnableVolumeCollisions": boolean(true), "TruckStaticFriction": float(0.5),
            "TruckDynamicFriction": float(0.4), "TruckRestitution": float(0.0)})),
        class("physics", serde_json::json!({
            "WorldGravity": {"type": "EA::Reflection::Vector4", "data": gravity},
            "FreezingEnergy": float(0.001)})),
        class("physicstrucks_drives", serde_json::json!({
            "Angular_Hard_Displacement": float(drives.angular_displacement),
            "Angular_Hard_Damping": float(drives.angular_damping),
            "Angular_Hard_Strength": float(drives.angular_strength)})),
        class("inputlistener", serde_json::json!({"StickMagnitudeMinToCountHeld": float(0.2)})),
        class("physics_push", serde_json::json!({"MaxPushableSpeed": float(8.0)})),
        class("physics_brakes", serde_json::json!({"FootBrakeForce": float(20.0), "SlowSpeed": float(0.1)})),
    ]});
    serde_json::from_value(json).expect("fixture matches the collections schema")
}

fn spawn() -> BoardSim {
    let mut sim = BoardSim::spawn(
        &fixture(),
        GtaVec::new(100.0, -200.0, 0.0),
        0.0,
        TestControlTuning::default(),
        PatchSettings::default(),
    )
    .unwrap();
    sim.settle(&mut Flat).unwrap();
    sim
}

fn run(sim: &mut BoardSim, ticks: u32, controls: Controls) {
    for _ in 0..ticks {
        sim.tick(controls, &mut Flat).unwrap();
    }
}

#[test]
fn board_settles_on_its_wheels() {
    let mut sim = spawn();
    run(&mut sim, 180, Controls::default());
    let deck = sim.deck_position_gta();
    assert!(sim.contact_count > 0, "wheels touch the sampled ground");
    assert!(deck.z > 0.02 && deck.z < 0.25, "deck height {}", deck.z);
    assert!(sim.speed() < 0.05, "resting speed {}", sim.speed());
    assert!((deck.x - 100.0).abs() < 0.05 && (deck.y + 200.0).abs() < 0.05);
}

#[test]
fn pushing_rolls_the_board_along_its_heading() {
    let mut sim = spawn();
    run(&mut sim, 60, Controls::default());
    let start = sim.deck_position_gta();
    run(&mut sim, 120, Controls { push: true, ..Default::default() });
    let moved = sim.deck_position_gta().sub(start);
    assert!(moved.y > 0.3, "heading 0 is GTA north, moved {moved:?}");
    assert!(moved.x.abs() < moved.y * 0.2, "rolls straight, moved {moved:?}");
    assert!(sim.speed() > 0.5);
}

#[test]
fn braking_slows_the_board() {
    let mut sim = spawn();
    run(&mut sim, 60, Controls::default());
    run(&mut sim, 120, Controls { push: true, ..Default::default() });
    let rolling = sim.speed();
    run(&mut sim, 60, Controls { brake: true, ..Default::default() });
    assert!(sim.speed() < rolling * 0.8, "{} -> {}", rolling, sim.speed());
}

#[test]
fn steering_turns_toward_the_stick() {
    let lateral = |steer: f32| {
        let mut sim = spawn();
        run(&mut sim, 60, Controls::default());
        run(&mut sim, 90, Controls { push: true, ..Default::default() });
        let start = sim.deck_position_gta();
        run(&mut sim, 120, Controls { push: true, steer, ..Default::default() });
        sim.deck_position_gta().sub(start).x
    };
    let left = lateral(-1.0);
    let right = lateral(1.0);
    assert!(left < 0.0 && right > 0.0, "heading north, left is west: {left} {right}");
    assert!((left - right).abs() > 0.1);
}

#[test]
fn a_tick_fits_the_frame_budget() {
    let mut sim = spawn();
    let start = std::time::Instant::now();
    run(&mut sim, 120, Controls { push: true, steer: 0.5, ..Default::default() });
    let per_tick = start.elapsed() / 120;
    println!("per tick {per_tick:?}");
    assert!(per_tick < std::time::Duration::from_millis(4), "{per_tick:?}");
}

#[test]
fn runs_within_a_small_script_fiber_stack() {
    // ScriptHookV script fibers get the game's default stack reservation.
    let worker = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let mut sim = spawn();
            run(&mut sim, 60, Controls { push: true, steer: 0.5, ..Default::default() });
            sim.speed()
        })
        .unwrap();
    assert!(worker.join().unwrap() > 0.0);
}
