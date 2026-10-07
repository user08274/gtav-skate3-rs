//! Replays the retarget on a ped skeleton and Skate 3 joints dumped by the
//! mod into SkateGTA.log (RETARGET_LOG=<log>), and draws them side by side
//! into RETARGET_PNG (front and side views); skipped without RETARGET_LOG.
//! SKATE_GTA_ASSETS supplies the Skate 3 bone order for the joint lines.
use skate_gta::{
    coords::GtaVec,
    pose::{BonePose, Rig},
};
use std::collections::HashMap;

pub struct Dump {
    pub rest: Vec<BonePose>,
    pub parents: Vec<i32>,
    pub tags: Vec<u16>,
    pub names: Vec<String>,
    /// Per logged tick: joint name -> (parent index in Skate order, model-space position).
    pub joints: Vec<(u32, HashMap<String, (i32, GtaVec)>)>,
}

fn floats(it: &mut std::str::SplitWhitespace, n: usize) -> Vec<f32> {
    (0..n).map(|_| it.next().unwrap().parse().unwrap()).collect()
}

pub fn parse(text: &str) -> Dump {
    // The last run in the log.
    let start = text.rfind("Ped pose mode:").expect("a ped pose run");
    let text = &text[start..];
    let (mut rest, mut parents, mut tags, mut names, mut joints) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for line in text.lines() {
        if let Some(body) = line.strip_prefix("Ped rest ") {
            let mut it = body.split_whitespace();
            let index: usize = it.next().unwrap().parse().unwrap();
            assert_eq!(index, rest.len());
            parents.push(it.next().unwrap().parse().unwrap());
            tags.push(u16::from_str_radix(it.next().unwrap(), 16).unwrap());
            names.push(it.next().unwrap().to_string());
            let mut v = |label: &str| {
                assert_eq!(it.next(), Some(label));
                let f = floats(&mut it, 3);
                GtaVec::new(f[0], f[1], f[2])
            };
            let position = v("p");
            let axes = [v("x"), v("y"), v("z")];
            rest.push(BonePose { axes, position });
        } else if let Some(body) = line.strip_prefix("Skate joints tick ") {
            let (tick, list) = body.split_once(": ").unwrap();
            let map = list
                .split("; ")
                .map(|entry| {
                    let mut it = entry.split_whitespace();
                    let name = it.next().unwrap().to_string();
                    let parent: i32 = it.next().unwrap().parse().unwrap();
                    let f = floats(&mut it, 3);
                    (name, (parent, GtaVec::new(f[0], f[1], f[2])))
                })
                .collect();
            joints.push((tick.parse().unwrap(), map));
        }
    }
    Dump { rest, parents, tags, names, joints }
}

pub fn rig(dump: &Dump) -> Rig {
    let tags = dump.tags.clone();
    Rig::new(dump.rest.clone(), dump.parents.clone(), move |tag| tags.iter().position(|&t| t as i32 == tag)).unwrap()
}

struct Canvas {
    w: u32,
    h: u32,
    px: Vec<u8>,
}

impl Canvas {
    fn new(w: u32, h: u32) -> Self {
        let mut px = vec![255u8; (w * h * 4) as usize];
        for p in px.chunks_mut(4) {
            p.copy_from_slice(&[245, 245, 245, 255]);
        }
        Self { w, h, px }
    }
    fn dot(&mut self, x: i32, y: i32, c: [u8; 3]) {
        if x >= 0 && y >= 0 && (x as u32) < self.w && (y as u32) < self.h {
            let i = ((y as u32 * self.w + x as u32) * 4) as usize;
            self.px[i..i + 3].copy_from_slice(&c);
        }
    }
    fn line(&mut self, a: (f32, f32), b: (f32, f32), c: [u8; 3]) {
        let n = ((b.0 - a.0).abs().max((b.1 - a.1).abs()) as i32).max(1);
        for i in 0..=n {
            let t = i as f32 / n as f32;
            let (x, y) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            for (dx, dy) in [(0, 0), (1, 0), (0, 1)] {
                self.dot(x as i32 + dx, y as i32 + dy, c);
            }
        }
    }
}

/// Panel `col` of a 3-wide, 2-high grid; row 0 front (x right, z up), row 1 side (y right, z up).
fn project(p: GtaVec, col: u32, row: u32) -> (f32, f32) {
    let (cw, ch, scale) = (300.0, 360.0, 160.0);
    let u = if row == 0 { p.x } else { p.y };
    (col as f32 * cw + cw / 2.0 + u * scale, row as f32 * ch + ch * 0.62 - p.z * scale)
}

fn draw_bones(c: &mut Canvas, points: &[GtaVec], parents: &[i32], keep: impl Fn(usize) -> bool, col: u32, color: [u8; 3]) {
    for row in 0..2 {
        for (i, &p) in parents.iter().enumerate() {
            if p < 0 || !keep(i) || !keep(p as usize) {
                continue;
            }
            c.line(project(points[i], col, row), project(points[p as usize], col, row), color);
        }
    }
}

/// Small axis ticks for selected bones (x red, y green, z blue).
fn draw_axes(c: &mut Canvas, poses: &[BonePose], bones: &[usize], col: u32) {
    for row in 0..2 {
        for &b in bones {
            let p = poses[b].position;
            for (k, color) in [[220, 40, 40], [40, 160, 40], [40, 40, 220]].into_iter().enumerate() {
                c.line(project(p, col, row), project(p.add(poses[b].axes[k].scale(0.06)), col, row), color);
            }
        }
    }
}

#[test]
fn replay_logged_retarget() {
    let Some(path) = std::env::var_os("RETARGET_LOG") else { return };
    let dump = parse(&std::fs::read_to_string(path).unwrap());
    assert_eq!(dump.rest.len(), 317);
    let rig = rig(&dump);
    eprintln!("{}", rig.describe());
    let skate_names: Option<Vec<String>> = std::env::var_os("SKATE_GTA_ASSETS").map(|root| {
        let game = skate_gameplay::host::Game::load(std::path::Path::new(&root), skate_gameplay::host::Mode::Easy).unwrap();
        game.bone_names().to_vec()
    });
    let skel = |i: usize| dump.names[i].starts_with("SKEL_");
    let key: Vec<usize> = ["SKEL_Pelvis", "SKEL_Spine3", "SKEL_Head", "SKEL_L_UpperArm", "SKEL_L_Forearm", "SKEL_R_Calf"]
        .iter()
        .filter_map(|n| dump.names.iter().position(|m| m == n))
        .collect();
    for (tick, joints) in &dump.joints {
        let solved = rig.solve(|name| joints.get(name).map(|j| j.1));
        let mut canvas = Canvas::new(900, 720);
        // Skate 3 joints.
        if let Some(names) = &skate_names {
            let points: Vec<GtaVec> = names.iter().map(|n| joints.get(n).map_or(GtaVec::default(), |j| j.1)).collect();
            let parents: Vec<i32> = names.iter().map(|n| joints.get(n).map_or(-1, |j| j.0)).collect();
            let body = |i: usize| {
                let n = names[i].as_str();
                n != "TRAJECTORY" && !n.ends_with("_REPARENTED") && !n.contains("WHEEL") && !n.starts_with("TRUCK") && n != "SKATEBOARD_ROOT"
            };
            draw_bones(&mut canvas, &points, &parents, body, 0, [200, 90, 0]);
        }
        let rest: Vec<GtaVec> = dump.rest.iter().map(|b| b.position).collect();
        draw_bones(&mut canvas, &rest, &dump.parents, skel, 1, [60, 60, 60]);
        draw_axes(&mut canvas, &dump.rest, &key, 1);
        let posed: Vec<GtaVec> = solved.iter().map(|b| b.position).collect();
        draw_bones(&mut canvas, &posed, &dump.parents, skel, 2, [30, 30, 160]);
        draw_axes(&mut canvas, &solved, &key, 2);
        if let Some(out) = std::env::var_os("RETARGET_PNG") {
            let out = std::path::PathBuf::from(out);
            let file = out.with_file_name(format!("{}-{tick}.png", out.file_stem().unwrap().to_string_lossy()));
            std::fs::write(&file, skate_gta::png::encode(canvas.w, canvas.h, &canvas.px)).unwrap();
        }
    }
}
