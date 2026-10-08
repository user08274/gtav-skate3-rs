//! Full Skate 3 gameplay on GTA-style ray-probed ground, with the player's
//! converted files (SKATE_GTA_ASSETS); skipped otherwise.
use skate_core::player::state::PhysicalStateId;
use skate_gameplay::host::Mode;
use skate_gta::{
    coords::GtaVec,
    ride::Ride,
    terrain::{GroundProbe, PatchSettings},
};
use std::time::Duration;

struct Street;
impl GroundProbe for Street {
    // A flat street at z = 31.5 with a 12 cm curb east of x = 105.
    fn down(&mut self, x: f32, _y: f32, top: f32, bottom: f32) -> Option<f32> {
        let z = if x > 105.0 { 31.62 } else { 31.5 };
        (top >= z && bottom <= z).then_some(z)
    }
}

const A: usize = 16;

fn frame() -> Duration {
    Duration::from_secs_f32(1.0 / 60.0)
}

#[test]
fn skater_rides_on_probed_gta_ground_along_the_player_heading() {
    let Some(root) = std::env::var_os("SKATE_GTA_ASSETS") else { return };
    let worker = std::thread::Builder::new()
        // Script fibers have small stacks; gameplay itself runs on its own thread.
        .stack_size(128 * 1024)
        .spawn(move || {
            let mut probe = Street;
            // Heading 90: the player faces west (-X).
            let mut ride = Ride::start(
                std::path::Path::new(&root),
                Mode::Easy,
                GtaVec::new(100.0, 200.0, 31.5),
                90.0,
                std::env::var("BIG").map_or(PatchSettings::default(), |_| PatchSettings { cells: 32, spacing: 2.0, ..PatchSettings::default() }),
                &mut probe,
            )
            .unwrap();
            for _ in 0..60 {
                ride.advance(frame(), 4, [0.0; 18], &mut probe).unwrap();
            }
            let hips = ride.hips_position();
            assert_eq!(ride.game.state(), PhysicalStateId::PhysicsGround);
            assert!(hips.z > 32.0 && hips.z < 33.0, "hips {hips:?}");
            let start = ride.deck_position();
            for i in 0..240 {
                let mut pad = [0.0; 18];
                pad[A] = if i % 60 < 30 { 1.0 } else { 0.0 };
                ride.advance(frame(), 4, pad, &mut probe).unwrap();
            }
            let moved = ride.deck_position().sub(start);
            assert!(moved.x < -5.0 && moved.y.abs() < moved.x.abs() * 0.3, "rides west: {moved:?}");
            assert!((ride.deck_position().z - 31.6).abs() < 0.3);
            let camera = ride.camera().expect("Skate 3 camera frame");
            let deck = ride.deck_position();
            assert!(camera.position.x - deck.x > 1.5, "camera follows from behind (east): {:?}", camera.position.sub(deck));
            assert!(camera.position.z - deck.z < 2.5, "camera stays low: {:?}", camera.position.sub(deck));
            assert!(camera.forward.x < -0.8, "camera looks ahead, not down: {:?}", camera.forward);
            moved
        })
        .unwrap();
    eprintln!("moved {:?}", worker.join().unwrap());
}
