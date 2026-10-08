//! Retarget the real Skate 3 skater (player data) onto an A-posed GTA-like
//! rig and check the result is a skating pose. Needs SKATE_GTA_ASSETS.
use skate_gameplay::host::Mode;
use skate_gta::{
    coords::{GtaVec, heading_degrees},
    pose::{BonePose, Rig},
    ride::Ride,
    terrain::{GroundProbe, PatchSettings},
};
use std::time::Duration;

struct Flat;
impl GroundProbe for Flat {
    fn down(&mut self, _x: f32, _y: f32, top: f32, bottom: f32) -> Option<f32> {
        (top >= 31.5 && bottom <= 31.5).then_some(31.5)
    }
}

fn v(x: f32, y: f32, z: f32) -> GtaVec {
    GtaVec::new(x, y, z)
}

/// GTA ped bind pose (A-pose, metres, model space with origin at the pelvis).
fn gta_rig() -> (Rig, Vec<i32>) {
    let bones: Vec<(i32, i32, GtaVec)> = vec![
        (0x0, -1, v(0.0, 0.0, 0.0)),
        (0x2E28, 0, v(0.0, 0.0, 0.0)),
        (0xE0FD, 0, v(0.0, 0.0, 0.0)),
        (0x5C01, 2, v(0.0, 0.0, 0.08)),
        (0x60F0, 3, v(0.0, 0.0, 0.2)),
        (0x60F1, 4, v(0.0, 0.0, 0.32)),
        (0x60F2, 5, v(0.0, 0.0, 0.45)),
        (0x9995, 6, v(0.0, 0.0, 0.6)),
        (0x796E, 7, v(0.0, 0.0, 0.7)),
        (0xFCD9, 6, v(-0.04, 0.0, 0.52)),
        (0xB1C5, 9, v(-0.18, 0.0, 0.5)),
        (0xEEEB, 10, v(-0.38, 0.0, 0.32)),
        (0x49D9, 11, v(-0.56, 0.0, 0.14)),
        (0x29D2, 6, v(0.04, 0.0, 0.52)),
        (0x9D4D, 13, v(0.18, 0.0, 0.5)),
        (0x6E5C, 14, v(0.38, 0.0, 0.32)),
        (0xDEAD, 15, v(0.56, 0.0, 0.14)),
        (0xE39F, 1, v(-0.1, 0.0, -0.05)),
        (0xF9BB, 17, v(-0.1, 0.0, -0.5)),
        (0x3779, 18, v(-0.1, 0.0, -0.92)),
        (0x083C, 19, v(-0.1, 0.13, -0.98)),
        (0xCA72, 1, v(0.1, 0.0, -0.05)),
        (0x9000, 21, v(0.1, 0.0, -0.5)),
        (0xCC4D, 22, v(0.1, 0.0, -0.92)),
        (0x512D, 23, v(0.1, 0.13, -0.98)),
    ];
    let identity = [v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0), v(0.0, 0.0, 1.0)];
    let rest = bones.iter().map(|b| BonePose { axes: identity, position: b.2 }).collect();
    let parents = bones.iter().map(|b| b.1).collect();
    let tags: Vec<i32> = bones.iter().map(|b| b.0).collect();
    let t = tags.clone();
    (Rig::new(rest, parents, move |tag| t.iter().position(|&x| x == tag)).unwrap(), tags)
}

#[test]
fn skater_pose_lands_on_a_gta_rig() {
    let Some(root) = std::env::var_os("SKATE_GTA_ASSETS") else { return };
    let mut probe = Flat;
    let mut ride = Ride::start(std::path::Path::new(&root), Mode::Easy, v(0.0, 0.0, 31.5), 0.0, PatchSettings::default(), &mut probe).unwrap();
    for i in 0..120 {
        let mut pad = [0.0; 18];
        pad[16] = if i % 40 < 20 { 1.0 } else { 0.0 };
        ride.advance(Duration::from_secs_f32(1.0 / 60.0), 4, pad, &mut probe).unwrap();
    }
    let view = ride.view();
    let names = ride.game.bone_names().to_vec();
    let heading = heading_degrees(view.axes.forward);
    let (s, c) = heading.to_radians().sin_cos();
    let origin = view.hips;
    let model = |p: GtaVec| {
        let d = p.sub(origin);
        v(d.x * c + d.y * s, -d.x * s + d.y * c, d.z)
    };
    let joints: std::collections::HashMap<&str, GtaVec> = names.iter().zip(&view.bones).map(|(n, (p, _))| (n.as_str(), model(*p))).collect();
    for k in ["HIPS", "HEAD", "LEFTUPLEG", "RIGHTUPLEG", "LEFTFOOT", "RIGHTFOOT", "LEFTHAND", "RIGHTHAND"] {
        eprintln!("{k:12} {:?}", joints[k]);
    }
    let (rig, tags) = gta_rig();
    let pose = rig.solve(|n| joints.get(n).copied(), |_| None);
    let at = |tag: i32| pose[tags.iter().position(|&t| t == tag).unwrap()].position;
    let (head, lfoot, rfoot, lhand) = (at(0x796E), at(0x3779), at(0xCC4D), at(0x49D9));
    eprintln!("GTA head {head:?} feet {lfoot:?} {rfoot:?} left hand {lhand:?}");
    let skate_feet = (joints["LEFTFOOT"], joints["RIGHTFOOT"]);
    // Feet spread along the board (model Y), as when skating, not side by side.
    assert!((lfoot.y - rfoot.y).abs() > 0.25, "stance along the board: {lfoot:?} {rfoot:?}");
    assert!(((lfoot.y - rfoot.y) - (skate_feet.0.y - skate_feet.1.y)).abs() < 0.2, "same stance as Skate 3");
    assert!(head.z > 0.5, "upright");
    assert!(lhand.z < 0.3, "arms are not left in the A-pose");
}
