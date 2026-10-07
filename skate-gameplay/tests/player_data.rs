//! Full gameplay pipeline on flat ground with the player's converted files.
//! Set SKATE_GTA_ASSETS to a skate3rust `assets` folder; skipped otherwise.
use skate_core::player::state::PhysicalStateId;
use skate_gameplay::host::{Game, Mode, SPAWN_GROUND_HEIGHT};
use std::path::PathBuf;

mod common;

const A: usize = 16;
const LEFT_STICK_X: usize = 0;

fn root() -> Option<PathBuf> {
    std::env::var_os("SKATE_GTA_ASSETS").map(PathBuf::from)
}

use common::flat_world as flat;

fn game(root: &PathBuf) -> Game {
    let mut game = Game::load(root, Mode::Easy).expect("gameplay loads from the player's files");
    game.set_world(flat(SPAWN_GROUND_HEIGHT)).unwrap();
    game
}

fn run(game: &mut Game, ticks: u32, pad: impl Fn(u32) -> [f32; 18]) {
    for i in 0..ticks {
        game.tick(pad(i)).unwrap_or_else(|e| panic!("tick {}: {e}", game.ticks()));
    }
}

#[test]
fn skater_stands_on_the_board_and_stays_on_the_ground() {
    let Some(root) = root() else { return };
    let mut game = game(&root);
    run(&mut game, 180, |_| [0.0; 18]);
    assert_eq!(game.state(), PhysicalStateId::PhysicsGround);
    assert!(game.wheel_contacts() > 0);
    let hips = game.hips_position();
    eprintln!("hips at {hips:?}");
    assert!(hips.y > 0.6 && hips.y < 1.3, "skater stands on the deck: hips at {hips:?}");
    assert!(game.board_speed() < 0.2, "resting speed {}", game.board_speed());
}

#[test]
fn pushing_with_a_accelerates_and_the_stick_turns() {
    let Some(root) = root() else { return };
    let mut game = game(&root);
    run(&mut game, 60, |_| [0.0; 18]);
    let start = game.deck().translation;
    // Tap A like a player pushing: half a second held, half released.
    run(&mut game, 240, |i| {
        let mut pad = [0.0; 18];
        pad[A] = if i % 60 < 30 { 1.0 } else { 0.0 };
        pad
    });
    let speed = game.board_speed();
    let end = game.deck().translation;
    let travelled = ((end.x - start.x).powi(2) + (end.z - start.z).powi(2)).sqrt();
    eprintln!("after pushing: speed {speed:.2} m/s, travelled {travelled:.2} m, state {:?}", game.state());
    assert!(speed > 1.0 && travelled > 2.0);
    let heading = |g: &Game| {
        let at = g.deck().basis.columns[2];
        at[0].atan2(at[2])
    };
    let before = heading(&game);
    run(&mut game, 90, |_| {
        let mut pad = [0.0; 18];
        pad[LEFT_STICK_X] = 1.0;
        pad
    });
    let turned = (heading(&game) - before).abs();
    eprintln!("turned {:.1} degrees, state {:?}", turned.to_degrees(), game.state());
    assert!(turned > 0.2, "the left stick steers");
}

const RIGHT_STICK_Y: usize = 4;

#[test]
fn flicking_the_right_stick_ollies() {
    let Some(root) = root() else { return };
    let mut game = game(&root);
    run(&mut game, 60, |_| [0.0; 18]);
    run(&mut game, 120, |i| {
        let mut pad = [0.0; 18];
        pad[A] = if i % 60 < 30 { 1.0 } else { 0.0 };
        pad
    });
    let ground_y = game.deck().translation.y;
    let mut states = Vec::new();
    let mut peak = ground_y;
    // Pull the right stick down, then flick it up: the Skate ollie.
    for i in 0..150u32 {
        let mut pad = [0.0; 18];
        pad[RIGHT_STICK_Y] = match i {
            0..12 => -1.0,
            12..16 => 1.0,
            _ => 0.0,
        };
        game.tick(pad).unwrap();
        states.push(game.state());
        peak = peak.max(game.deck().translation.y);
    }
    eprintln!("ollie peak {:.2} m above the ground, states {:?}", peak - ground_y, {
        let mut s = states.clone();
        s.dedup();
        s
    });
    assert!(peak - ground_y > 0.1, "the deck leaves the ground");
}

#[test]
fn a_tick_fits_a_frame_and_a_script_fiber_stack() {
    let Some(root) = root() else { return };
    let worker = std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(move || {
            let mut game = game(&root);
            run(&mut game, 60, |_| [0.0; 18]);
            let start = std::time::Instant::now();
            run(&mut game, 240, |i| {
                let mut pad = [0.0; 18];
                pad[A] = if i % 60 < 30 { 1.0 } else { 0.0 };
                pad[LEFT_STICK_X] = 0.5;
                pad
            });
            start.elapsed() / 240
        })
        .unwrap();
    let per_tick = worker.join().expect("runs within a 1 MB stack");
    eprintln!("per tick {per_tick:?}");
    assert!(per_tick < std::time::Duration::from_millis(4));
}


const Y: usize = 15;

#[test]
fn y_steps_off_the_board_and_back_on() {
    let Some(root) = root() else { return };
    let mut game = game(&root);
    run(&mut game, 60, |_| [0.0; 18]);
    let mut states = Vec::new();
    let tap_y = |game: &mut Game, states: &mut Vec<_>| {
        for i in 0..180u32 {
            let mut pad = [0.0; 18];
            pad[Y] = if i < 8 { 1.0 } else { 0.0 };
            game.tick(pad).unwrap();
            states.push(game.state());
        }
    };
    tap_y(&mut game, &mut states);
    let off = game.state();
    let pose = game.render_pose().len();
    tap_y(&mut game, &mut states);
    states.dedup();
    eprintln!("states {states:?}; after first Y {off:?}; posed bones {pose}");
    assert!(states.iter().any(|s| matches!(s, PhysicalStateId::BipedGround | PhysicalStateId::OffBoardPushing)));
}

#[test]
fn skeleton_stands_over_the_deck() {
    let Some(root) = root() else { return };
    let mut game = game(&root);
    run(&mut game, 120, |_| [0.0; 18]);
    let names = game.bone_names().to_vec();
    let bones = game.skeleton_world();
    let at = |name: &str| bones[names.iter().position(|n| n == name).unwrap()].0;
    let (head, hips, foot, deck) = (at("HEAD"), at("HIPS"), at("LEFTFOOT"), game.deck().translation);
    eprintln!("head {head:?} hips {hips:?} foot {foot:?} deck {deck:?} size {:?}", game.deck_size());
    assert!(head.y > hips.y && hips.y > foot.y, "upright skeleton");
    assert!(head.y - deck.y > 1.2 && head.y - deck.y < 2.0);
    assert!((foot.y - deck.y).abs() < 0.25, "feet on the deck");
    let [w, l] = game.deck_size();
    assert!(w > 0.15 && w < 0.35 && l > 0.6 && l < 1.0);
}

/// Body spins as Skate 3 does them: the left stick held to one side through
/// the ollie (and in the air) turns the skater; prints the yaw turned in the
/// air and the trick names the scoring shows.
#[test]
fn left_stick_through_an_ollie_spins() {
    let Some(root) = root() else { return };
    let yaw = |game: &Game| {
        let f = game.deck().basis.columns[2];
        f[0].atan2(f[2]).to_degrees()
    };
    for (label, timing) in [("held from the crouch", 0u32..80), ("pushed at the flick", 12..80), ("only in the air", 20..80)] {
        let mut game = game(&root);
        run(&mut game, 60, |_| [0.0; 18]);
        run(&mut game, 120, |i| {
            let mut pad = [0.0; 18];
            pad[A] = if i % 60 < 30 { 1.0 } else { 0.0 };
            pad
        });
        let mut names: Vec<String> = Vec::new();
        let (mut turned, mut last) = (0.0f32, yaw(&game));
        for i in 0..200u32 {
            let mut pad = [0.0; 18];
            pad[RIGHT_STICK_Y] = match i {
                0..12 => -1.0,
                12..16 => 1.0,
                _ => 0.0,
            };
            if timing.contains(&i) {
                pad[LEFT_STICK_X] = 1.0;
            }
            game.tick(pad).unwrap();
            let now = yaw(&game);
            if !matches!(game.state(), PhysicalStateId::PhysicsGround) {
                turned += (now - last + 540.0).rem_euclid(360.0) - 180.0;
            }
            last = now;
            let name = game.trick_name().to_string();
            if names.last() != Some(&name) {
                names.push(name);
            }
        }
        eprintln!("left stick {label}: turned {turned:.0} deg in the air, tricks {names:?}");
        assert!(turned.abs() > 90.0, "{label}: the skater spins");
        assert!(names.iter().any(|n| n.ends_with(" 180") || n.ends_with(" 360")), "{label}: {names:?}");
    }
}
