//! The Skate 3 board model from the player's converted skater.glb
//! (SKATE_GTA_ASSETS/private/skater.glb); skipped without it.
use skate_gameplay::host::Mode;
use skate_gta::{
    board_model::BoardModel,
    coords::GtaVec,
    ride::Ride,
    terrain::{GroundProbe, PatchSettings},
};
use std::time::Duration;

const STREET: f32 = 31.5;

struct Street;
impl GroundProbe for Street {
    fn down(&mut self, _x: f32, _y: f32, top: f32, bottom: f32) -> Option<f32> {
        (top >= STREET && bottom <= STREET).then_some(STREET)
    }
}

#[test]
fn the_board_model_sits_on_the_physics_board() {
    let Some(root) = std::env::var_os("SKATE_GTA_ASSETS").map(std::path::PathBuf::from) else { return };
    let Ok(bytes) = std::fs::read(root.join("private/skater.glb")) else { return };
    let model = BoardModel::load(&bytes).unwrap();
    let small = model.simplified(0.012, &["SKATEBOARD_ROOT"]);
    eprintln!(
        "board bones {:?}; {} triangles, {} after merging truck and wheel detail",
        model.bones,
        model.triangles.len(),
        small.triangles.len()
    );
    assert!(model.bones.iter().any(|b| b == "SKATEBOARD_ROOT") && model.bones.len() >= 7);
    let worker = std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(move || {
            let mut probe = Street;
            let mut ride = Ride::start(&root, Mode::Easy, GtaVec::new(100.0, 200.0, STREET), 30.0, PatchSettings::default(), &mut probe).unwrap();
            for i in 0..150u32 {
                let mut pad = [0.0; 18];
                pad[16] = if i % 60 < 30 { 1.0 } else { 0.0 };
                ride.advance(Duration::from_secs_f32(1.0 / 60.0), 4, pad, &mut probe).unwrap();
            }
            let view = ride.view();
            let names = ride.game.bone_names().to_vec();
            let frame = |bone: &str| {
                let i = names.iter().position(|n| n == bone)?;
                Some((view.bones[i].0, view.bone_axes[i]))
            };
            let faces = small.place(frame);
            assert_eq!(faces.len(), small.triangles.len(), "every board bone is in the render pose");
            let points: Vec<GtaVec> = faces.iter().flat_map(|f| f.corners).collect();
            let lowest = points.iter().map(|p| p.z).fold(f32::MAX, f32::min);
            let n = points.len() as f32;
            let centre = points.iter().fold(GtaVec::default(), |a, p| a.add(*p)).scale(1.0 / n);
            let deck = view.deck;
            eprintln!("model centre {centre:?}, physics deck {deck:?}, lowest point {lowest:.3} (street {STREET})");
            let d = centre.sub(deck);
            assert!((d.x * d.x + d.y * d.y).sqrt() < 0.05, "model centred on the deck: {d:?}");
            assert!((lowest - STREET).abs() < 0.03, "wheels touch the street: {lowest}");
            // Long axis along the board's forward axis.
            let along = |p: &GtaVec| { let q = p.sub(deck); q.x * view.axes.forward.x + q.y * view.axes.forward.y };
            let length = points.iter().map(along).fold(f32::MIN, f32::max) - points.iter().map(along).fold(f32::MAX, f32::min);
            assert!((length - 0.9).abs() < 0.1, "about 0.9 m long along the board: {length}");
            let mean: Vec<f32> = (0..3).map(|k| faces.iter().map(|f| f.color[k]).sum::<f32>() / faces.len() as f32).collect();
            eprintln!("mean colour {mean:?}");
            if let Some(out) = std::env::var_os("BOARD_PNG") {
                std::fs::write(out, render(&faces, deck, view.axes)).unwrap();
            }
        })
        .unwrap();
    worker.join().unwrap();
}

/// Flat-shaded software render of the faces: top, side and bottom views.
fn render(faces: &[skate_gta::board_model::Face], centre: GtaVec, axes: skate_gta::coords::EntityAxes) -> Vec<u8> {
    let (w, h) = (900u32, 900u32);
    let mut px = vec![0u8; (w * h * 4) as usize];
    let mut depth = vec![f32::MAX; (w * h) as usize];
    for p in px.chunks_mut(4) {
        p.copy_from_slice(&[200, 210, 220, 255]);
    }
    let light = GtaVec::new(0.3, -0.4, 0.85).normalized();
    // Views: (screen right, screen up, towards viewer) in board axes.
    let views = [
        (axes.forward, axes.right.scale(-1.0), axes.up, 150.0f32),
        (axes.forward, axes.up, axes.right, 450.0),
        (axes.forward, axes.right, axes.up.scale(-1.0), 750.0),
    ];
    for (right, up, toward, cy) in views {
        for f in faces {
            if f.normal.x * toward.x + f.normal.y * toward.y + f.normal.z * toward.z <= 0.0 {
                continue;
            }
            let shade = 0.45 + 0.55 * (f.normal.x * light.x + f.normal.y * light.y + f.normal.z * light.z).max(0.0);
            let s = f.corners.map(|c| {
                let d = c.sub(centre);
                let dot = |a: GtaVec| d.x * a.x + d.y * a.y + d.z * a.z;
                (450.0 + dot(right) * 900.0, cy - dot(up) * 900.0, -dot(toward))
            });
            let (x0, x1) = (s.iter().map(|p| p.0).fold(f32::MAX, f32::min).max(0.0) as u32, (s.iter().map(|p| p.0).fold(f32::MIN, f32::max) as u32 + 1).min(w));
            let (y0, y1) = (s.iter().map(|p| p.1).fold(f32::MAX, f32::min).max(0.0) as u32, (s.iter().map(|p| p.1).fold(f32::MIN, f32::max) as u32 + 1).min(h));
            let area = (s[1].0 - s[0].0) * (s[2].1 - s[0].1) - (s[2].0 - s[0].0) * (s[1].1 - s[0].1);
            if area.abs() < 1e-6 {
                continue;
            }
            for y in y0..y1 {
                for x in x0..x1 {
                    let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                    let e = |a: usize, b: usize| (s[b].0 - s[a].0) * (fy - s[a].1) - (s[b].1 - s[a].1) * (fx - s[a].0);
                    let (w0, w1, w2) = (e(1, 2) / area, e(2, 0) / area, e(0, 1) / area);
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    let z = w0 * s[0].2 + w1 * s[1].2 + w2 * s[2].2;
                    let i = (y * w + x) as usize;
                    if z < depth[i] {
                        depth[i] = z;
                        for k in 0..3 {
                            px[i * 4 + k] = (f.color[k] * shade * 255.0).clamp(0.0, 255.0) as u8;
                        }
                    }
                }
            }
        }
    }
    skate_gta::png::encode(w, h, &px)
}
