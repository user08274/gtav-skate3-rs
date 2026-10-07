//! Runs against the player's converted Skate 3 files when SKATE_GTA_ASSETS
//! points at a skate3rust `assets` folder; skipped otherwise. EA data is
//! never part of this repository.
use skate_core::physics::{
    drive_frames::default_body_transforms,
    mass::default_skateboard_mass_properties,
};
use skate_data::collections::Collections;
use skate_gta::{
    assets,
    coords::GtaVec,
    settings::PhysicsSettings,
    sim::{BoardSim, Controls, TestControlTuning},
    terrain::{GroundProbe, PatchSettings},
};
use std::path::PathBuf;

fn data() -> Option<Collections> {
    let root = PathBuf::from(std::env::var_os("SKATE_GTA_ASSETS")?);
    let root = assets::resolve(Some(&root), None).expect("SKATE_GTA_ASSETS is a converted assets folder");
    Some(assets::load_collections(&root).expect("collections load"))
}

struct Flat;
impl GroundProbe for Flat {
    fn down(&mut self, _x: f32, _y: f32, top: f32, bottom: f32) -> Option<f32> {
        (top >= 0.0 && bottom <= 0.0).then_some(0.0)
    }
}

struct Hill;
impl GroundProbe for Hill {
    // 8% downhill toward GTA north (+Y).
    fn down(&mut self, _x: f32, y: f32, top: f32, bottom: f32) -> Option<f32> {
        let z = -0.08 * y;
        (top >= z && bottom <= z).then_some(z)
    }
}

fn spawn(data: &Collections, probe: &mut dyn GroundProbe, z: f32) -> BoardSim {
    let mut sim = BoardSim::spawn(
        data,
        GtaVec::new(0.0, 0.0, z),
        0.0,
        TestControlTuning::default(),
        PatchSettings::default(),
    )
    .unwrap();
    sim.settle(probe).unwrap();
    sim
}

#[test]
fn stock_disc_board_matches_the_recovered_retail_assembly() {
    let Some(data) = data() else { return };
    let settings = PhysicsSettings::load(&data).unwrap();
    assert_eq!(settings.masses, default_skateboard_mass_properties());
    assert_eq!(settings.authored, default_body_transforms());
}

#[test]
fn board_rests_rolls_brakes_and_turns_on_player_data() {
    let Some(data) = data() else { return };
    let mut sim = spawn(&data, &mut Flat, 0.0);
    for _ in 0..120 {
        sim.tick(Controls::default(), &mut Flat).unwrap();
    }
    assert!(sim.speed() < 0.02, "rest {}", sim.speed());
    let start = sim.deck_position_gta();
    for _ in 0..120 {
        sim.tick(Controls { push: true, ..Default::default() }, &mut Flat).unwrap();
    }
    assert!(sim.deck_position_gta().sub(start).y > 0.3);
    let rolling = sim.speed();
    for _ in 0..60 {
        sim.tick(Controls { brake: true, ..Default::default() }, &mut Flat).unwrap();
    }
    assert!(sim.speed() < rolling * 0.8);
    let before = sim.deck_position_gta();
    for _ in 0..120 {
        sim.tick(Controls { push: true, steer: 1.0, ..Default::default() }, &mut Flat).unwrap();
    }
    assert!(sim.deck_position_gta().sub(before).x > 0.0, "right stick turns east from north");
}

#[test]
fn board_rolls_down_a_hill_by_gravity() {
    let Some(data) = data() else { return };
    let mut sim = spawn(&data, &mut Hill, 0.0);
    let start = sim.deck_position_gta();
    for _ in 0..180 {
        sim.tick(Controls::default(), &mut Hill).unwrap();
    }
    let moved = sim.deck_position_gta().sub(start);
    assert!(moved.y > 0.5 && moved.z < 0.0, "rolls downhill: {moved:?}");
}
