//! Presentation must move smoothly when GTA's frame rate does not match the
//! 60 Hz gameplay tick. Needs SKATE_GTA_ASSETS.
use skate_gameplay::host::Mode;
use skate_gta::{coords::GtaVec, ride::Ride, terrain::{GroundProbe, PatchSettings}};
use std::time::Duration;

struct Flat;
impl GroundProbe for Flat {
    fn down(&mut self, _x: f32, _y: f32, top: f32, bottom: f32) -> Option<f32> {
        (top >= 31.5 && bottom <= 31.5).then_some(31.5)
    }
}

#[test]
fn interpolated_board_speed_is_steady_at_uneven_frame_rates() {
    let Some(root) = std::env::var_os("SKATE_GTA_ASSETS") else { return };
    let worker = std::thread::Builder::new().stack_size(128 * 1024).spawn(move || {
        let mut probe = Flat;
        let mut ride = Ride::start(std::path::Path::new(&root), Mode::Easy, GtaVec::new(0.0, 0.0, 31.5), 0.0, PatchSettings::default(), &mut probe).unwrap();
        for i in 0..240 {
            let mut pad = [0.0; 18];
            pad[16] = if i % 40 < 20 { 1.0 } else { 0.0 };
            ride.advance(Duration::from_secs_f32(1.0 / 60.0), 4, pad, &mut probe).unwrap();
        }
        // Coast while GTA renders at an uneven ~62 fps.
        let mut rates = Vec::new();
        let mut last = ride.view().deck;
        for i in 0..240 {
            let dt = [0.0161, 0.0158, 0.0167, 0.0149, 0.0172][i % 5];
            ride.advance(Duration::from_secs_f32(dt), 4, [0.0; 18], &mut probe).unwrap();
            let now = ride.view().deck;
            let d = now.sub(last);
            rates.push((d.x * d.x + d.y * d.y).sqrt() / dt);
            last = now;
        }
        rates
    }).unwrap();
    let rates = worker.join().unwrap();
    let (lo, hi) = rates[10..].iter().fold((f32::MAX, 0.0f32), |(lo, hi), &r| (lo.min(r), hi.max(r)));
    eprintln!("presented speed {:.2}..{:.2} m/s", lo, hi);
    assert!(lo > 0.0, "no frozen frames");
    assert!(hi / lo < 1.35, "no doubled steps: {lo:.2}..{hi:.2}");
}
