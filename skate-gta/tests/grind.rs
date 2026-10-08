//! Grinding GTA geometry with the player's converted files (SKATE_GTA_ASSETS);
//! skipped otherwise. A 30 cm ledge runs north beside the spawn; the skater
//! pushes north, ollies and steers onto it.
use skate_core::player::state::PhysicalStateId;
use skate_gameplay::host::Mode;
use skate_gta::{
    coords::GtaVec,
    ride::Ride,
    terrain::{GroundProbe, PatchSettings},
};
use std::time::Duration;

pub const STREET: f32 = 31.5;
pub const LEDGE_TOP: f32 = 31.8;
/// The ledge occupies x in [LEDGE_X, LEDGE_X + 1.5], y in [190, 300].
pub const LEDGE_X: f32 = 100.55;

pub struct Ledge;
impl GroundProbe for Ledge {
    fn down(&mut self, x: f32, y: f32, top: f32, bottom: f32) -> Option<f32> {
        let on_ledge = (LEDGE_X..LEDGE_X + 1.5).contains(&x) && (190.0..300.0).contains(&y);
        let z = if on_ledge { LEDGE_TOP } else { STREET };
        (top >= z && bottom <= z).then_some(z)
    }
}

const A: usize = 16;
const LEFT_STICK_X: usize = 0;
const RIGHT_STICK_Y: usize = 4;

pub fn frame() -> Duration {
    Duration::from_secs_f32(1.0 / 60.0)
}

/// Pushes north, then ollies with the left stick toward the ledge; returns
/// the physical states seen from the ollie on.
pub fn approach_and_ollie(ride: &mut Ride, probe: &mut dyn GroundProbe, steer: f32, push_ticks: u32) -> (Vec<PhysicalStateId>, u32) {
    for _ in 0..30 {
        ride.advance(frame(), 4, [0.0; 18], probe).unwrap();
    }
    for i in 0..push_ticks {
        let mut pad = [0.0; 18];
        pad[A] = if i % 60 < 30 { 1.0 } else { 0.0 };
        ride.advance(frame(), 4, pad, probe).unwrap();
    }
    let mut states = Vec::new();
    let mut grind_frames = 0;
    let mut streak = 0;
    let mut longest = 0;
    for i in 0..240u32 {
        let mut pad = [0.0; 18];
        pad[RIGHT_STICK_Y] = match i {
            0..10 => -1.0,
            10..14 => 1.0,
            _ => 0.0,
        };
        if (8..40).contains(&i) {
            pad[LEFT_STICK_X] = steer;
        }
        ride.advance(frame(), 4, pad, probe).unwrap();
        let state = ride.game.state();
        if (400..=405).contains(&(state as u32)) {
            grind_frames += 1;
            streak += 1;
            longest = longest.max(streak);
        } else {
            streak = 0;
        }
        if states.last() != Some(&state) {
            eprintln!("frame {i} state {state:?} deck {:?}", ride.deck_position());
            states.push(state);
        }
    }
    eprintln!("grind frames {grind_frames}, longest {longest}, final deck {:?}", ride.deck_position());
    (states, longest)
}

pub fn grinding(states: &[PhysicalStateId]) -> bool {
    states.iter().any(|s| (400..=405).contains(&(*s as u32)))
}

#[test]
fn a_ledge_edge_given_by_hand_can_be_ground() {
    let Some(root) = std::env::var_os("SKATE_GTA_ASSETS") else { return };
    let worker = std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(move || {
            let mut results = Vec::new();
            for steer in [1.0f32, -1.0] {
                let mut probe = Ledge;
                let mut ride = Ride::start(
                    std::path::Path::new(&root),
                    Mode::Easy,
                    GtaVec::new(100.0, 200.0, STREET),
                    0.0,
                    PatchSettings::default(),
                    &mut probe,
                )
                .unwrap();
                let rail: Vec<_> = [195.0, 230.0, 299.0]
                    .iter()
                    .map(|&y| ride.frame.to_skate(GtaVec::new(LEDGE_X, y, LEDGE_TOP)))
                    .collect();
                ride.game.set_grind_rails(&[rail]).unwrap();
                let (states, longest) = approach_and_ollie(&mut ride, &mut probe, steer, 90);
                eprintln!("steer {steer}: {states:?} deck x {:.2}", ride.deck_position().x);
                results.push(grinding(&states) && longest >= 60);
            }
            assert!(results.iter().any(|&g| g), "one approach must sustain a grind for at least one second");
        })
        .unwrap();
    worker.join().unwrap();
}

#[test]
fn ledges_found_in_the_ground_can_be_ground() {
    let Some(root) = std::env::var_os("SKATE_GTA_ASSETS") else { return };
    let worker = std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(move || {
            let mut results = Vec::new();
            for steer in [1.0f32, -1.0] {
                let mut probe = Ledge;
                let mut ride = Ride::start(
                    std::path::Path::new(&root),
                    Mode::Easy,
                    GtaVec::new(100.0, 200.0, STREET),
                    0.0,
                    PatchSettings::default(),
                    &mut probe,
                )
                .unwrap();
                let (states, longest) = approach_and_ollie(&mut ride, &mut probe, steer, 90);
                let lines = ride.grind_lines();
                eprintln!("steer {steer}: {states:?}; {} grind lines, first {:?}", lines.len(), lines.first());
                assert!(lines.iter().any(|l| l.iter().all(|p| (p.x - LEDGE_X).abs() < 0.02 && (p.z - LEDGE_TOP).abs() < 1e-3)),
                    "the ledge's west edge is a grind line: {lines:?}");
                results.push(grinding(&states) && longest >= 60);
            }
            assert!(results.iter().any(|&g| g), "one approach must sustain a grind for at least one second");
        })
        .unwrap();
    worker.join().unwrap();
}
