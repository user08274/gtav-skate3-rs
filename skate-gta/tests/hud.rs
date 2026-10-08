//! The Skate 3 scoring HUD inside a GTA ride, with the player's converted
//! files (SKATE_GTA_ASSETS); skipped otherwise. HUD_PREVIEW=<file.png> also
//! renders the busiest HUD frame the way drawTexture places it on 1920x1080.
use skate_gameplay::{host::Mode, hud::Sprite};
use skate_gta::{
    coords::GtaVec,
    hud_draw::{crop, place, SpriteKey},
    ride::Ride,
    terrain::{GroundProbe, PatchSettings},
};
use std::{collections::HashMap, time::Duration};

struct Flat;
impl GroundProbe for Flat {
    fn down(&mut self, _x: f32, _y: f32, top: f32, bottom: f32) -> Option<f32> {
        (top >= 10.0 && bottom <= 10.0).then_some(10.0)
    }
}

const RIGHT_STICK_Y: usize = 4;
const A: usize = 16;

#[test]
fn an_ollie_lights_up_the_hud_during_a_ride() {
    let Some(root) = std::env::var_os("SKATE_GTA_ASSETS") else { return };
    let worker = std::thread::Builder::new()
        .stack_size(128 * 1024)
        .spawn(move || {
            let mut probe = Flat;
            let frame = Duration::from_secs_f32(1.0 / 60.0);
            let mut ride = Ride::start(
                std::path::Path::new(&root),
                Mode::Easy,
                GtaVec::new(0.0, 0.0, 10.0),
                0.0,
                PatchSettings::default(),
                &mut probe,
            )
            .unwrap();
            assert!(ride.hud.is_some(), "HUD loads: {:?}", ride.hud_error);
            for i in 0..180 {
                let mut pad = [0.0; 18];
                pad[A] = if i >= 60 && i % 60 < 30 { 1.0 } else { 0.0 };
                ride.advance(frame, 4, pad, &mut probe).unwrap();
            }
            let idle = ride.hud_sprites().len();
            let mut busiest: Vec<Sprite> = Vec::new();
            for i in 0..240 {
                let mut pad = [0.0; 18];
                pad[RIGHT_STICK_Y] = match i {
                    0..12 => -1.0,
                    12..16 => 1.0,
                    _ => 0.0,
                };
                ride.advance(frame, 4, pad, &mut probe).unwrap();
                if ride.hud_sprites().len() > busiest.len() {
                    busiest = ride.hud_sprites().to_vec();
                }
            }
            assert!(ride.hud_error.is_none(), "{:?}", ride.hud_error);
            assert!(busiest.len() > idle, "idle {idle}, busiest {}", busiest.len());
            let hud = ride.hud.as_ref().unwrap();
            let mut atlases = HashMap::new();
            let mut images = HashMap::new();
            for sprite in &busiest {
                let key = SpriteKey::of(sprite);
                let size = hud.texture_size(&key.texture).unwrap();
                let atlas = atlases.entry(key.texture.clone()).or_insert_with(|| hud.texture_rgba(&key.texture).unwrap());
                let image = crop(atlas, size, &key).unwrap();
                images.insert(key, image);
            }
            if let Some(path) = std::env::var_os("HUD_PREVIEW") {
                let (w, h) = (1920u32, 1080u32);
                let mut canvas = vec![0u8; (w * h * 4) as usize];
                for px in canvas.chunks_mut(4) {
                    px.copy_from_slice(&[70, 80, 90, 255]);
                }
                for sprite in &busiest {
                    let (iw, ih, pixels) = &images[&SpriteKey::of(sprite)];
                    let p = place(sprite, w as f32 / h as f32);
                    blit(&mut canvas, w, h, *iw, *ih, pixels, &p);
                }
                std::fs::write(path, skate_gta::png::encode(w, h, &canvas)).unwrap();
            }
            eprintln!("HUD sprites idle {idle}, busiest {}", busiest.len());
        })
        .unwrap();
    worker.join().unwrap();
}

/// What drawTexture does: sizes in screen widths, centre position, clockwise
/// rotation in turns, straight-alpha blending of texture x colour.
fn blit(canvas: &mut [u8], w: u32, h: u32, iw: u32, ih: u32, pixels: &[u8], p: &skate_gta::hud_draw::Placement) {
    let (sw, sh) = (p.size[0] * w as f32, p.size[1] * w as f32);
    let (cx, cy) = (p.position[0] * w as f32, p.position[1] * h as f32);
    let (s, c) = (p.rotation * std::f32::consts::TAU).sin_cos();
    let reach = (sw * sw + sh * sh).sqrt() * 0.5 + 1.0;
    let (x0, x1) = ((cx - reach).max(0.0) as u32, ((cx + reach) as u32).min(w));
    let (y0, y1) = ((cy - reach).max(0.0) as u32, ((cy + reach) as u32).min(h));
    for y in y0..y1 {
        for x in x0..x1 {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            // Undo the clockwise rotation (y down).
            let (lx, ly) = (dx * c + dy * s, -dx * s + dy * c);
            let (u, v) = (lx / sw + 0.5, ly / sh + 0.5);
            if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
                continue;
            }
            let t = (((v * ih as f32) as u32 * iw + (u * iw as f32) as u32) * 4) as usize;
            let a = pixels[t + 3] as f32 / 255.0 * p.color[3];
            let o = ((y * w + x) * 4) as usize;
            for i in 0..3 {
                let src = pixels[t + i] as f32 * p.color[i];
                canvas[o + i] = (src * a + canvas[o + i] as f32 * (1.0 - a)).round() as u8;
            }
        }
    }
}
