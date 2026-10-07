//! Riding off an 8 m drop with the player's converted files
//! (SKATE_GTA_ASSETS); skipped otherwise. The ground under a high jump is
//! out of reach for a moment, which once ended the session.
use skate_core::player::state::PhysicalStateId;
use skate_gameplay::host::Mode;
use skate_gta::{
    coords::GtaVec,
    ride::Ride,
    terrain::{GroundProbe, PatchSettings},
};
use std::time::Duration;

struct Cliff;
impl GroundProbe for Cliff {
    fn down(&mut self, _x: f32, y: f32, top: f32, bottom: f32) -> Option<f32> {
        let z = if y < 206.0 { 31.5 } else { 23.5 };
        (top >= z && bottom <= z).then_some(z)
    }
}

#[test]
fn riding_off_a_drop_keeps_the_session_and_lands_below() {
    drop_with(PatchSettings::default());
}

/// Rays too short to see the bottom while falling: the old ground stays
/// until the new one comes into reach.
#[test]
fn short_rays_over_a_drop_keep_the_session() {
    drop_with(PatchSettings { probe_below: 3.0, ..PatchSettings::default() });
}

fn drop_with(patch: PatchSettings) {
    let Some(root) = std::env::var_os("SKATE_GTA_ASSETS") else { return };
    let worker = std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(move || {
            let mut probe = Cliff;
            let frame = Duration::from_secs_f32(1.0 / 60.0);
            let mut ride = Ride::start(
                std::path::Path::new(&root),
                Mode::Easy,
                GtaVec::new(100.0, 200.0, 31.5),
                0.0,
                patch,
                &mut probe,
            )
            .unwrap();
            let mut lowest = f32::MAX;
            let mut airborne = 0;
            for i in 0..600u32 {
                let mut pad = [0.0; 18];
                pad[16] = if i < 240 && i % 60 < 30 { 1.0 } else { 0.0 };
                ride.advance(frame, 4, pad, &mut probe).expect("the session survives the drop");
                lowest = lowest.min(ride.deck_position().z);
                if !matches!(ride.game.state(), PhysicalStateId::PhysicsGround) {
                    airborne += 1;
                }
            }
            eprintln!("deck lowest z {lowest:.2}, final {:?} at {:?}, {airborne} frames off the ground", ride.game.state(), ride.deck_position());
            assert!(ride.deck_position().y > 206.0, "went over the edge");
            assert!((ride.deck_position().z - 23.6).abs() < 0.5, "landed below: {:?}", ride.deck_position());
        })
        .unwrap();
    worker.join().unwrap();
}
