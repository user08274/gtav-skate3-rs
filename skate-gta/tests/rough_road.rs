//! GTA asphalt is never flat: ride the full gameplay over a gently uneven
//! road and look for sudden speed drops. Needs SKATE_GTA_ASSETS.
use skate_gameplay::host::Mode;
use skate_gta::{coords::GtaVec, ride::Ride, terrain::{GroundProbe, PatchSettings}};
use std::time::Duration;

struct Road;
impl GroundProbe for Road {
    fn down(&mut self, x: f32, y: f32, top: f32, bottom: f32) -> Option<f32> {
        let grain = ((x * 12.9898 + y * 78.233).sin() * 43758.545).fract() * 0.004;
        let z = 31.5 + 0.03 * (x * 0.45).sin() + 0.02 * (y * 1.1).sin() + 0.01 * (x * 2.3 + y).sin() + grain;
        (top >= z && bottom <= z).then_some(z)
    }
}

#[test]
fn speed_does_not_collapse_on_uneven_asphalt() {
    let Some(root) = std::env::var_os("SKATE_GTA_ASSETS") else { return };
    let worker = std::thread::Builder::new().stack_size(128 * 1024).spawn(move || {
        let mut probe = Road;
        let mut ride = Ride::start(std::path::Path::new(&root), Mode::Easy, GtaVec::new(100.0, 200.0, 31.5), 90.0, PatchSettings::default(), &mut probe).unwrap();
        let mut speeds = Vec::new();
        let mut cam_up = 0.0f32;
        for i in 0..600usize {
            let mut pad = [0.0; 18];
            pad[16] = if i < 360 && i % 40 < 20 { 1.0 } else { 0.0 };
            let dt = Duration::from_secs_f32(if i % 3 == 0 { 1.0 / 30.0 } else { 1.0 / 62.0 });
            ride.advance(dt, 4, pad, &mut probe).unwrap();
            speeds.push(ride.game.board_speed() * 3.6);
            if i > 120 {
                let c = ride.camera().unwrap();
                cam_up = cam_up.max(c.position.z - ride.deck_position().z);
            }
        }
        let worst_drop = speeds.windows(2).map(|w| w[0] - w[1]).fold(0.0f32, f32::max);
        let top = speeds[300..360].iter().copied().fold(0.0f32, f32::max);
        (top, worst_drop, speeds[599], cam_up, format!("{:?}", ride.game.state()))
    }).unwrap();
    let (top, worst_drop, end, cam_up, state) = worker.join().unwrap();
    eprintln!("top {top:.1} km/h, worst single-frame drop {worst_drop:.2} km/h, after coasting {end:.1} km/h, camera max {cam_up:.2} m above deck, {state}");
    assert!(top > 27.0, "reaches Skate 3 push speed");
    assert!(worst_drop < 4.0, "no sudden speed loss (deck speed wobbles a little on bumps)");
    assert!(cam_up < 3.0, "camera stays behind, not overhead");
}
