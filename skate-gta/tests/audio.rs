//! The Skate 3 sounds of a ride, rendered headless with the player's converted
//! files (SKATE_GTA_ASSETS, with private/audio); skipped otherwise. AUDIO_WAV=<file>
//! writes the render. The skater stands, pushes north along a ledge, ollies onto
//! it and grinds: standing is quiet, rolling sounds, the grind adds to it.
use skate_gameplay::{game_audio::GameAudio, host::Mode};
use skate_gta::{
    coords::GtaVec,
    ride::Ride,
    terrain::{GroundProbe, PatchSettings},
};
use std::time::Duration;

const STREET: f32 = 31.5;
const LEDGE_TOP: f32 = 31.8;
const LEDGE_X: f32 = 100.55;

struct Ledge;
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
const FRAME: f32 = 1.0 / 60.0;
const SAMPLES_PER_FRAME: usize = 800;

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

fn write_wav(path: &std::ffi::OsStr, stereo: &[f32]) {
    let mut out = Vec::new();
    let data = (stereo.len() * 2) as u32;
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&[1, 0, 2, 0]);
    out.extend_from_slice(&48000u32.to_le_bytes());
    out.extend_from_slice(&(48000u32 * 4).to_le_bytes());
    out.extend_from_slice(&[4, 0, 16, 0]);
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for v in stereo {
        out.extend_from_slice(&((v.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, out).unwrap();
}

#[test]
fn a_ride_sounds_like_skate_3() {
    let Some(root) = std::env::var_os("SKATE_GTA_ASSETS") else { return };
    let root = std::path::PathBuf::from(root);
    if !root.join("private/audio/audio_manifest.json").is_file() {
        return;
    }
    std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(move || {
            let started = std::time::Instant::now();
            let mut audio = GameAudio::start(&root).unwrap();
            eprintln!("audio start {:?}: {:?}", started.elapsed(), audio.report);
            let runtime = audio.runtime();
            let mut probe = Ledge;
            let mut ride = Ride::start(&root, Mode::Easy, GtaVec::new(100.0, 200.0, STREET), 0.0, PatchSettings::default(), &mut probe).unwrap();
            let mut out = Vec::new();
            let mut segments: Vec<(&str, std::ops::Range<usize>)> = Vec::new();
            let mut states = Vec::new();
            let mut grind_frames = 0;
            let mut cost = Duration::ZERO;
            let mut step = |ride: &mut Ride, pad: [f32; 18], out: &mut Vec<f32>| {
                ride.advance(Duration::from_secs_f32(FRAME), 4, pad, &mut probe).unwrap();
                let t = std::time::Instant::now();
                ride.audio_frame(&mut audio, None, FRAME);
                let mut buf = vec![0.0f32; SAMPLES_PER_FRAME * 2];
                runtime.lock().unwrap().fill_stereo(&mut buf);
                cost += t.elapsed();
                out.extend_from_slice(&buf);
            };
            let mark = |out: &Vec<f32>| out.len();
            // The spawn drop lands the board (a touchdown); then the skater stands.
            for _ in 0..60 {
                step(&mut ride, [0.0; 18], &mut out);
            }
            let a = mark(&out);
            for _ in 0..30 {
                step(&mut ride, [0.0; 18], &mut out);
            }
            segments.push(("standing", a..mark(&out)));
            let a = mark(&out);
            for i in 0..150u32 {
                let mut pad = [0.0; 18];
                pad[A] = if i % 60 < 30 { 1.0 } else { 0.0 };
                step(&mut ride, pad, &mut out);
            }
            segments.push(("pushing", a..mark(&out)));
            let a = mark(&out);
            for i in 0..240u32 {
                let mut pad = [0.0; 18];
                pad[RIGHT_STICK_Y] = match i {
                    0..10 => -1.0,
                    10..14 => 1.0,
                    _ => 0.0,
                };
                if (8..40).contains(&i) {
                    pad[LEFT_STICK_X] = 0.6;
                }
                step(&mut ride, pad, &mut out);
                let state = ride.game.state();
                grind_frames += u32::from(ride.game.grinding());
                if states.last() != Some(&state) {
                    states.push(state);
                }
            }
            segments.push(("ollie, grind, land", a..mark(&out)));
            let frames = out.len() / (SAMPLES_PER_FRAME * 2);
            eprintln!("states {states:?}, grind frames {grind_frames}, audio host + render per frame {:?}", cost / frames as u32);
            let mut levels = Vec::new();
            for (name, range) in &segments {
                let level = rms(&out[range.clone()]);
                eprintln!("{name}: rms {level:.4}");
                levels.push(level);
            }
            if let Some(path) = std::env::var_os("AUDIO_WAV") {
                write_wav(&path, &out);
            }
            assert!(out.iter().all(|v| v.is_finite()));
            assert!(levels[1] > 0.005, "rolling is audible ({})", levels[1]);
            assert!(levels[1] > levels[0] * 3.0, "rolling is louder than standing");
            assert!(levels[2] > 0.005, "the trick part is audible ({})", levels[2]);
            assert!(cost / (frames as u32) < Duration::from_millis(4), "audio stays cheap: {:?}", cost / frames as u32);
        })
        .unwrap()
        .join()
        .unwrap();
}
