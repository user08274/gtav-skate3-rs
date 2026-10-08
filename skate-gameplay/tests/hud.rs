//! HUD sprite recovery, plus the scoring HUD on the player's converted files
//! (set SKATE_GTA_ASSETS to a skate3rust `assets` folder; skipped otherwise).
mod common;

use skate_gameplay::{
    apt_scene::{Draw, Vertex},
    host::{Game, Mode, SPAWN_GROUND_HEIGHT},
    hud::{sprites_of, Hud, Sprite},
};

fn sprite(draw: &Draw, size: [u32; 2]) -> Option<Sprite> {
    let mut all = sprites_of(draw, size);
    assert!(all.len() <= 1);
    all.pop()
}

fn quad(p: [[f32; 2]; 4], uv: [[f32; 2]; 4]) -> Draw {
    let v = |i: usize| Vertex { position: p[i], uv: uv[i] };
    Draw { texture: "t".into(), vertices: vec![v(0), v(1), v(2), v(0), v(2), v(3)], multiply: [1.0; 4], add: [0.0; 4] }
}

#[test]
fn upright_quad_becomes_an_unrotated_sprite() {
    let s = sprite(&quad([[10., 20.], [74., 20.], [74., 52.], [10., 52.]], [[0., 0.], [1., 0.], [1., 1.], [0., 1.]]), [64, 32]).unwrap();
    assert_eq!(s.center, [42., 36.]);
    assert_eq!(s.size, [64., 32.]);
    assert_eq!(s.region, [0, 0, 64, 32]);
    assert!(!s.mirrored && s.rotation_degrees.abs() < 1e-4);
}

#[test]
fn mirrored_and_partial_regions_are_recovered() {
    let s = sprite(&quad([[0., 0.], [20., 0.], [20., 10.], [0., 10.]], [[0.5, 0.25], [0.25, 0.25], [0.25, 0.5], [0.5, 0.5]]), [128, 128]).unwrap();
    assert!(s.mirrored);
    assert_eq!(s.region, [32, 32, 64, 64]);
    assert!(s.rotation_degrees.abs() < 1e-4);
}

#[test]
fn rotated_quad_reports_its_angle() {
    let s = sprite(&quad([[0., 0.], [0., 10.], [-5., 10.], [-5., 0.]], [[0., 0.], [1., 0.], [1., 1.], [0., 1.]]), [16, 8]).unwrap();
    assert!((s.rotation_degrees - 90.0).abs() < 1e-3, "{}", s.rotation_degrees);
    assert_eq!(s.size, [10., 5.]);
}

#[test]
fn text_draws_split_into_one_sprite_per_glyph() {
    let mut text = quad([[0., 0.], [8., 0.], [8., 12.], [0., 12.]], [[0., 0.], [0.5, 0.], [0.5, 1.], [0., 1.]]);
    let next = quad([[9., 0.], [17., 0.], [17., 12.], [9., 12.]], [[0.5, 0.], [1., 0.], [1., 1.], [0.5, 1.]]);
    text.vertices.extend(next.vertices);
    let glyphs = sprites_of(&text, [16, 12]);
    assert_eq!(glyphs.len(), 2);
    assert_eq!(glyphs[1].region, [8, 0, 16, 12]);
    assert_eq!(glyphs[1].center, [13., 6.]);
}

#[test]
fn an_ollie_shows_up_on_the_scoring_hud() {
    let Some(root) = std::env::var_os("SKATE_GTA_ASSETS").map(std::path::PathBuf::from) else { return };
    let mut game = Game::load(&root, Mode::Easy).unwrap();
    game.set_world(common::flat_world(SPAWN_GROUND_HEIGHT)).unwrap();
    let mut hud = Hud::load(&root, &game).expect("HUD loads from the player's files");
    let tick = |game: &mut Game, hud: &mut Hud, pad: [f32; 18]| {
        game.tick(pad).unwrap();
        hud.update(game).unwrap();
    };
    for i in 0..180u32 {
        let mut pad = [0.0; 18];
        pad[16] = if i >= 60 && i % 60 < 30 { 1.0 } else { 0.0 };
        tick(&mut game, &mut hud, pad);
    }
    let idle = hud.sprites().unwrap().len();
    let mut most = 0;
    let mut textures = std::collections::BTreeSet::new();
    for i in 0..240u32 {
        let mut pad = [0.0; 18];
        pad[4] = match i {
            0..12 => -1.0,
            12..16 => 1.0,
            _ => 0.0,
        };
        tick(&mut game, &mut hud, pad);
        if let Some(name) = game.announced_trick() { eprintln!("tick {i}: new trick {name}"); }
        let sprites = hud.sprites().unwrap();
        for s in &sprites {
            assert!(s.size[0] > 0.0 && s.center[0].is_finite());
            assert!(hud.texture_size(&s.texture).is_some());
            textures.insert(s.texture.clone());
        }
        most = most.max(sprites.len());
    }
    eprintln!("HUD sprites idle {idle}, during the ollie up to {most}, textures {textures:?}");
    assert!(most > idle, "the trick is displayed");
    for t in &textures {
        let [w, h] = hud.texture_size(t).unwrap();
        assert_eq!(hud.texture_rgba(t).unwrap().len(), (w * h * 4) as usize);
    }
}
