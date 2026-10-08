//! ScriptHookV script: DllMain registration, the script fiber loop and one
//! riding session (board prop, attached player, fixed-step simulation).
mod hud;
mod natives;
mod shv;
mod entities;
mod hook;
mod skeleton;
mod watch;

use crate::{
    assets,
    config::{self, Config},
    coords::{self, GtaVec},
    ride::Ride,
    sim::{BoardSim, Controls},
    terrain::GroundProbe,
};
use natives as n;
use skate_core::physics::board::BodyId;
use skate_data::collections::Collections;
use std::{
    io::Write,
    path::PathBuf,
    sync::atomic::{AtomicPtr, Ordering},
};
use windows_sys::Win32::{
    Foundation::{HMODULE, MAX_PATH},
    System::{LibraryLoader::GetModuleFileNameW, SystemServices::{DLL_PROCESS_ATTACH, DLL_PROCESS_DETACH}},
    UI::Input::KeyboardAndMouse::GetAsyncKeyState,
};

fn length(v: GtaVec) -> f32 {
    (v.x * v.x + v.y * v.y + v.z * v.z).sqrt()
}

/// Git commit the .asi was built from.
const BUILD: &str = env!("SKATEGTA_BUILD");

static MODULE: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

#[unsafe(no_mangle)]
pub extern "system" fn DllMain(module: HMODULE, reason: u32, reserved: *mut core::ffi::c_void) -> i32 {
    match reason {
        DLL_PROCESS_ATTACH => {
            MODULE.store(module, Ordering::SeqCst);
            if let Err(error) = shv::register(module, script_main) {
                log(&format!("Cannot register with ScriptHookV: {error}"));
                return 0;
            }
        }
        DLL_PROCESS_DETACH => {
            shv::unregister(module);
            // Unloaded while the game keeps running: memcpy must not jump
            // into freed code. (At process exit nothing runs any more.)
            if reserved.is_null() {
                hook::uninstall();
            }
        }
        _ => {}
    }
    1
}

fn module_dir() -> PathBuf {
    let mut buffer = [0u16; MAX_PATH as usize * 4];
    let len = unsafe { GetModuleFileNameW(MODULE.load(Ordering::SeqCst), buffer.as_mut_ptr(), buffer.len() as u32) };
    let path = PathBuf::from(String::from_utf16_lossy(&buffer[..len as usize]));
    path.parent().map(PathBuf::from).unwrap_or_default()
}

fn log(message: &str) {
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(module_dir().join("SkateGTA.log"))
    {
        let _ = writeln!(file, "{message}");
    }
}

unsafe extern "C" fn script_main() {
    let outcome = std::panic::catch_unwind(run);
    if let Err(panic) = outcome {
        let text = panic
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_else(|| "unknown panic".into());
        log(&format!("SkateGTA stopped: {text}"));
    }
    loop {
        shv::wait(0);
    }
}

fn load_config() -> Config {
    let path = module_dir().join("SkateGTA.ini");
    match std::fs::read_to_string(&path) {
        Ok(text) => Config::parse(&text).unwrap_or_else(|error| {
            log(&error);
            n::notify(&format!("SkateGTA: {error}"));
            Config::default()
        }),
        Err(_) => {
            let _ = std::fs::write(&path, config::TEMPLATE);
            log(&format!("Created {}", path.display()));
            Config::parse(config::TEMPLATE).unwrap_or_default()
        }
    }
}

enum Active {
    Board(Session),
    Full(RideSession),
}

impl Active {
    fn update(&mut self, config: &Config) -> Result<(), String> {
        match self {
            Self::Board(s) => s.update(config),
            Self::Full(s) => s.update(config),
        }
    }
    fn end(self) {
        match self {
            Self::Board(s) => s.end(),
            Self::Full(s) => s.end(),
        }
    }
}

fn start(config: &Config, data: &mut Option<Collections>) -> Result<Active, String> {
    if config.full_gameplay {
        let root = assets::resolve(config.asset_root.as_deref(), config.skate3rust_dir.as_deref())?;
        RideSession::start(config, &root).map(Active::Full)
    } else {
        let data = load_data(config, data)?;
        Session::start(config, data).map(Active::Board)
    }
}

fn run() {
    let config = load_config();
    log(&format!("SkateGTA loaded (build {BUILD}, PedPoseMode {:?})", config.pose_mode));
    let mut data: Option<Collections> = None;
    let mut session: Option<Active> = None;
    let mut key_was_down = false;
    loop {
        let key_down = unsafe { GetAsyncKeyState(config.toggle_key as i32) } as u16 & 0x8000 != 0;
        let toggled = key_down && !key_was_down;
        key_was_down = key_down;
        if toggled {
            match session.take() {
                Some(s) => s.end(),
                None => match start(&config, &mut data) {
                    Ok(s) => session = Some(s),
                    Err(error) => {
                        log(&error);
                        n::notify(&format!("SkateGTA: {error}"));
                    }
                },
            }
        }
        if let Some(s) = session.as_mut() {
            if let Err(reason) = s.update(&config) {
                if !reason.is_empty() {
                    log(&reason);
                    n::notify(&format!("SkateGTA: {reason}"));
                }
                if let Some(s) = session.take() {
                    s.end();
                }
            }
        }
        shv::wait(0);
    }
}

fn load_data<'a>(config: &Config, cache: &'a mut Option<Collections>) -> Result<&'a Collections, String> {
    if cache.is_none() {
        let root = assets::resolve(config.asset_root.as_deref(), config.skate3rust_dir.as_deref())?;
        log(&format!("Skate 3 data: {}", root.display()));
        *cache = Some(assets::load_collections(&root)?);
    }
    Ok(cache.as_ref().expect("loaded above"))
}

struct GtaProbe {
    flags: i32,
    ignore: n::Entity,
}

impl GroundProbe for GtaProbe {
    fn down(&mut self, x: f32, y: f32, top: f32, bottom: f32) -> Option<f32> {
        n::probe(GtaVec::new(x, y, top), GtaVec::new(x, y, bottom), self.flags, self.ignore).map(|p| p.z)
    }
    fn toward(&mut self, from: GtaVec, to: GtaVec) -> Option<(GtaVec, GtaVec)> {
        n::probe_with_normal(from, to, self.flags, self.ignore)
    }
}

/// GTA control actions: INPUT_SPRINT, INPUT_JUMP, INPUT_ENTER, INPUT_MOVE_LR.
const PUSH: i32 = 21;
const BRAKE: i32 = 22;
const DISMOUNT: i32 = 23;
const STEER: i32 = 30;
const BLOCKED: [i32; 13] = [21, 22, 23, 24, 25, 30, 31, 36, 44, 140, 141, 142, 143];

struct Session {
    sim: BoardSim,
    ped: n::Entity,
    prop: Option<n::Entity>,
    probe: GtaProbe,
}

impl Session {
    fn start(config: &Config, data: &Collections) -> Result<Self, String> {
        let ped = n::player_ped_id();
        if n::is_entity_dead(ped) || n::is_ped_in_any_vehicle(ped) {
            return Err("get out of the vehicle first".into());
        }
        let position = n::get_entity_coords(ped);
        let heading = n::get_entity_heading(ped);
        let mut probe = GtaProbe { flags: config.probe_flags, ignore: ped };
        let ground = probe
            .down(position.x, position.y, position.z + 1.0, position.z - 3.0)
            .ok_or("no ground under the player")?;
        let mut sim = BoardSim::spawn(
            data,
            GtaVec::new(position.x, position.y, ground),
            heading,
            config.tuning,
            config.patch,
        )?;
        sim.settle(&mut probe).map_err(|e| format!("board failed to settle: {e:?}"))?;
        let prop = spawn_prop(config, sim.deck_position_gta());
        // The rider stays upright and only follows the board's heading; deck
        // roll from leaning trucks must not tip the whole ped over.
        n::freeze_entity_position(ped, true);
        n::set_ped_can_ragdoll(ped, false);
        let mut session = Self { sim, ped, prop, probe };
        session.present(config);
        n::notify("SkateGTA: on board");
        Ok(session)
    }

    fn update(&mut self, config: &Config) -> Result<(), String> {
        if !n::does_entity_exist(self.ped) || n::is_entity_dead(self.ped) {
            return Err(String::new());
        }
        for action in BLOCKED {
            n::disable_control_action(action);
        }
        if n::is_disabled_control_just_pressed(DISMOUNT) {
            return Err(String::new());
        }
        let controls = Controls {
            steer: n::get_disabled_control_normal(STEER),
            push: n::is_disabled_control_pressed(PUSH),
            brake: n::is_disabled_control_pressed(BRAKE),
        };
        self.sim
            .advance(n::get_frame_time(), 4, controls, &mut self.probe)
            .map_err(|e| format!("physics reset ({e:?})"))?;
        self.present(config);
        Ok(())
    }

    fn present(&mut self, config: &Config) {
        let deck = self.sim.deck();
        let axes = coords::entity_axes(deck.basis);
        let position = self.sim.frame.to_gta(deck.translation).add(axes.up.scale(config.model_z_offset));
        if let Some(prop) = self.prop {
            let q = coords::quaternion_mul(coords::quaternion(&axes), coords::yaw_quaternion(config.model_yaw_offset));
            n::set_entity_coords_no_offset(prop, position);
            n::set_entity_quaternion(prop, q);
        }
        let center = self.sim.frame.to_gta(deck.translation);
        n::set_entity_coords_no_offset(self.ped, center.add(GtaVec::new(0.0, 0.0, config.ped_z_offset)));
        n::set_entity_heading(self.ped, coords::heading_degrees(axes.forward));
        if config.debug_draw {
            self.draw_debug();
        }
        n::draw_text(
            &format!(
                "SkateGTA  {:.1} km/h  contacts {}  [A/Shift push, X/Space brake, Y/F off]",
                self.sim.speed() * 3.6,
                self.sim.contact_count
            ),
            0.01,
            0.01,
        );
    }

    fn draw_debug(&self) {
        let parts = self.sim.board.part_transforms();
        let at = |id: BodyId| self.sim.frame.to_gta(parts[id.index()].translation);
        let wheels = [BodyId::RightFrontWheel, BodyId::LeftFrontWheel, BodyId::LeftBackWheel, BodyId::RightBackWheel];
        for i in 0..4 {
            n::draw_line(at(wheels[i]), at(wheels[(i + 1) % 4]), [255, 200, 0, 255]);
        }
        let deck = self.sim.deck();
        let axes = coords::entity_axes(deck.basis);
        let center = self.sim.frame.to_gta(deck.translation);
        n::draw_line(center, center.add(axes.forward.scale(0.5)), [0, 160, 255, 255]);
        n::draw_line(center, center.add(axes.up.scale(0.3)), [0, 255, 0, 255]);
        if let Some(patch) = self.sim.patch() {
            for t in &patch.triangles {
                let v = t.triangle.vertices.map(|p| self.sim.frame.to_gta(p));
                for i in 0..3 {
                    n::draw_line(v[i], v[(i + 1) % 3], [80, 255, 80, 120]);
                }
            }
        }
    }

    fn end(self) {
        let landing = self.sim.deck_position_gta().add(GtaVec::new(0.0, 0.0, 1.0));
        if let Some(prop) = self.prop {
            n::delete_entity(prop);
        }
        n::freeze_entity_position(self.ped, false);
        n::set_ped_can_ragdoll(self.ped, true);
        n::set_entity_coords(self.ped, landing);
        n::notify("SkateGTA: off board");
    }
}

/// Frontend (pad-native) controls in Skate 3 gameplay action order.
const FRONTEND: i32 = 2;
const PAD_ACTIONS: [(i32, f32); 18] = [
    (218, 1.0),  // left stick X
    (219, -1.0), // left stick Y (GTA: down positive)
    (209, 1.0),  // L3
    (220, 1.0),  // right stick X
    (221, -1.0), // right stick Y
    (210, 1.0),  // R3
    (207, 1.0),  // LT
    (208, 1.0),  // RT
    (205, 1.0),  // LB
    (206, 1.0),  // RB
    (172, 1.0),  // d-pad up
    (173, 1.0),  // d-pad down
    (174, 1.0),  // d-pad left
    (175, 1.0),  // d-pad right
    (203, 1.0),  // X
    (204, 1.0),  // Y
    (201, 1.0),  // A
    (202, 1.0),  // B
];
const LEAVE: i32 = 217; // Back / Select

fn read_pad() -> [f32; 18] {
    PAD_ACTIONS.map(|(action, sign)| {
        let v = n::get_disabled_control_normal_in(FRONTEND, action) * sign;
        if v.is_finite() { v.clamp(-1.0, 1.0) } else { 0.0 }
    })
}

/// Drives the player ped's skeleton with the Skate 3 pose.
/// Skate 3 bone axes by name, turned into a ped model frame with `heading`.
fn model_axes<'a>(view: &crate::ride::View, names: &'a [String], heading: f32) -> std::collections::HashMap<&'a str, [GtaVec; 3]> {
    let (s, c) = heading.to_radians().sin_cos();
    let turn = |d: GtaVec| GtaVec::new(d.x * c + d.y * s, -d.x * s + d.y * c, d.z);
    names.iter().zip(&view.bone_axes).map(|(name, axes)| (name.as_str(), axes.map(turn))).collect()
}

struct Poser {
    rig: crate::pose::Rig,
    frames: std::cell::Cell<u32>,
    ticks: std::cell::Cell<u32>,
    written: std::cell::RefCell<Vec<crate::pose::BonePose>>,
    /// How the pose is put over GTA's; None = script writes only.
    watch: Option<watch::Mode>,
}

const POSE_LOG_FRAMES: u32 = 120;
/// Frames whose matrix accesses the watch records (after things settle).
const PROBE_FRAMES: std::ops::Range<u32> = 30..33;

fn largest_change(a: &[crate::pose::BonePose], b: &[crate::pose::BonePose]) -> f32 {
    a.iter()
        .zip(b)
        .flat_map(|(x, y)| {
            let d = |u: GtaVec, v: GtaVec| u.sub(v);
            [d(x.position, y.position), d(x.axes[0], y.axes[0]), d(x.axes[1], y.axes[1]), d(x.axes[2], y.axes[2])]
        })
        .map(|v| v.x.abs().max(v.y.abs()).max(v.z.abs()))
        .fold(0.0, f32::max)
}

impl Poser {
    fn new(ped: n::Entity, mode: config::PoseMode) -> Result<Self, String> {
        let skeleton = skeleton::PedSkeleton::find(ped)?;
        let watch = match mode {
            config::PoseMode::Hook => Some(watch::Mode::Hook),
            config::PoseMode::Guard => Some(watch::Mode::Guard),
            config::PoseMode::Script => None,
        };
        let watch = watch.filter(|&mode| match Self::watch(&skeleton, mode) {
            Ok(()) => true,
            Err(e) => {
                log(&format!("Ped pose watch unavailable: {e}"));
                false
            }
        });
        let index_of = |tag: i32| {
            let i = n::get_ped_bone_index(ped, tag);
            (i >= 0).then_some(i as usize)
        };
        let rig = crate::pose::Rig::new(skeleton.read(), skeleton.parents.clone(), index_of)?;
        log(&format!("Ped pose rig: {} | {}", rig.describe(), skeleton.describe));
        log(&format!("Ped pose mode: {watch:?} (None = script writes only)"));
        // Rest skeleton for offline retarget checks: index parent tag name, position, axes.
        let f = |v: GtaVec| format!("{:.5} {:.5} {:.5}", v.x, v.y, v.z);
        for (i, bone) in rig.rest().iter().enumerate() {
            log(&format!(
                "Ped rest {i} {} {:04X} {} p {} x {} y {} z {}",
                skeleton.parents[i],
                skeleton.tags.get(i).copied().unwrap_or(0),
                skeleton.names.get(i).map(|n| n.replace(' ', "_")).filter(|n| !n.is_empty()).unwrap_or("-".into()),
                f(bone.position),
                f(bone.axes[0]),
                f(bone.axes[1]),
                f(bone.axes[2])
            ));
        }
        Ok(Self { rig, frames: Default::default(), ticks: Default::default(), written: Default::default(), watch })
    }

    fn watch(skeleton: &skeleton::PedSkeleton, mode: watch::Mode) -> Result<(), String> {
        let own = skeleton::image_range(MODULE.load(Ordering::SeqCst) as *const u8);
        watch::start(skeleton.objects(), skeleton.locals, skeleton.len(), true, own, skeleton::game_range(), mode)
    }

    /// `origin`/`heading` are the transform just given to the ped.
    fn apply(&self, ped: n::Entity, view: &crate::ride::View, names: &[String], origin: GtaVec, heading: f32) -> Result<(), String> {
        // The script's own reads and writes must not hit the guard pages.
        watch::disarm();
        let tick = self.ticks.get();
        self.ticks.set(tick + 1);
        let guard = self.watch == Some(watch::Mode::Guard);
        if guard && tick == PROBE_FRAMES.end {
            for line in watch::report(&watch::records(), skeleton::game_range()) {
                log(&line);
            }
        }
        if self.watch == Some(watch::Mode::Hook) && (tick < 8 || tick % 120 == 0) {
            let [last, any, locals_last, locals] = watch::copies();
            log(&format!(
                "Ped pose hook tick {tick}: memcpy into final matrices {last} complete / {any} any, into locals {locals_last} complete / {locals} any; {} overrides since last log, frame {:.1} ms",
                watch::applied(),
                n::get_frame_time() * 1000.0
            ));
        }
        if guard && (tick < 8 || tick % 120 == 0) {
            log(&format!(
                "Ped pose watch tick {tick}: {} guard faults, {} overrides, {} frames over budget since last tick, frame {:.1} ms",
                watch::faults(),
                watch::applied(),
                watch::over_budget(),
                n::get_frame_time() * 1000.0
            ));
        }
        let skeleton = skeleton::PedSkeleton::find(ped)?;
        if let Some(mode) = self.watch {
            if skeleton.objects() as usize != watch::watched_objects() {
                log("Ped pose watch: the skeleton moved, watching the new one");
                Self::watch(&skeleton, mode)?;
            }
        }
        if skeleton.len() != self.rig.bone_count() {
            return Err("ped skeleton changed".into());
        }
        let (s, c) = heading.to_radians().sin_cos();
        let model = |p: GtaVec| {
            let d = p.sub(origin);
            GtaVec::new(d.x * c + d.y * s, -d.x * s + d.y * c, d.z)
        };
        let joints: std::collections::HashMap<&str, GtaVec> =
            names.iter().zip(&view.bones).map(|(name, (p, _))| (name.as_str(), model(*p))).collect();
        let axes = model_axes(view, names, heading);
        let pose = self.rig.solve(|name| joints.get(name).copied(), |name| axes.get(name).copied());
        if matches!(tick, 150 | 300 | 450 | 600) {
            let mut list: Vec<String> = names
                .iter()
                .zip(&view.bones)
                .map(|(name, (p, parent))| {
                    let q = model(*p);
                    let a = axes.get(name.as_str()).map_or(String::new(), |a| {
                        a.iter().map(|v| format!(" {:.4} {:.4} {:.4}", v.x, v.y, v.z)).collect()
                    });
                    format!("{name} {parent} {:.5} {:.5} {:.5}{a}", q.x, q.y, q.z)
                })
                .collect();
            list.sort();
            log(&format!("Skate joints tick {tick}: {}", list.join("; ")));
        }
        let frame = self.frames.get();
        if frame < POSE_LOG_FRAMES {
            let live = skeleton.read();
            let written = self.written.borrow();
            let changed = (!written.is_empty()).then(|| largest_change(&live, &written));
            let mirror = skeleton.read_mirror().map(|m| (largest_change(&m, self.rig.rest()), (!written.is_empty()).then(|| largest_change(&m, &written))));
            if frame < 3 {
                let key = ["HIPS", "SPINE", "HEAD", "LEFTUPLEG", "RIGHTUPLEG", "LEFTARM", "LEFTFOREARM", "LEFTHAND"];
                let targets: Vec<_> = key.iter().map(|k| format!("{k}={:?}", joints.get(k))).collect();
                log(&format!("Ped pose frame {frame}: joints {}", targets.join(" ")));
            }
            if frame < 6 || frame % 20 == 0 {
                log(&format!(
                    "Ped pose frame {frame}: frag copy changed by game {:?}, vs rest {:.3}; draw-handler copy (vs rest, vs our write) {:?}; solved vs rest {:.3}",
                    changed,
                    largest_change(&live, self.rig.rest()),
                    mirror,
                    largest_change(&pose, self.rig.rest()),
                ));
            }
            self.frames.set(frame + 1);
        }
        skeleton.write(&pose);
        if self.watch.is_some() {
            let rows = |pose: &[crate::pose::BonePose]| -> Vec<[f32; 12]> {
                pose.iter()
                    .map(|b| {
                        let v = [b.axes[0], b.axes[1], b.axes[2], b.position];
                        std::array::from_fn(|k| match k % 3 {
                            0 => v[k / 3].x,
                            1 => v[k / 3].y,
                            _ => v[k / 3].z,
                        })
                    })
                    .collect()
            };
            let locals = crate::pose::to_locals(&pose, &skeleton.parents);
            watch::set_pose(&rows(&pose), &rows(&locals));
            watch::arm(tick, PROBE_FRAMES.contains(&tick));
        }
        if self.frames.get() <= POSE_LOG_FRAMES {
            *self.written.borrow_mut() = pose;
        }
        Ok(())
    }
}

struct RideSession {
    ride: Ride,
    ped: n::Entity,
    prop: Option<n::Entity>,
    probe: GtaProbe,
    cam: Option<n::Cam>,
    poser: Option<Poser>,
    draw_body: bool,
    frames: u32,
    entities: entities::Entities,
    collide_entities: bool,
}

impl RideSession {
    fn start(config: &Config, root: &std::path::Path) -> Result<Self, String> {
        let ped = n::player_ped_id();
        if n::is_entity_dead(ped) || n::is_ped_in_any_vehicle(ped) {
            return Err("get out of the vehicle first".into());
        }
        let position = n::get_entity_coords(ped);
        let heading = n::get_entity_heading(ped);
        let mut probe = GtaProbe { flags: config.probe_flags, ignore: ped };
        let ground = probe
            .down(position.x, position.y, position.z + 1.0, position.z - 3.0)
            .ok_or("no ground under the player")?;
        log(&format!("Skate 3 data: {}", root.display()));
        let ride = Ride::start(
            root,
            config.mode,
            GtaVec::new(position.x, position.y, ground),
            heading,
            config.patch,
            &mut probe,
        )?;
        let mut ride = ride;
        ride.game.set_low_camera(config.low_camera);
        ride.grind_edges = config.grind_edges;
        ride.find_walls = config.collide_walls;
        let prop = if config.board_model.eq_ignore_ascii_case("none") { None } else { spawn_prop(config, ride.deck_position()) };
        if config.ped_freeze {
            n::freeze_entity_position(ped, true);
        } else {
            n::set_entity_has_gravity(ped, false);
            n::set_ped_gravity(ped, false);
            n::set_entity_collision(ped, false, false);
        }
        n::set_ped_can_ragdoll(ped, false);
        // The rider is driven by Skate 3: GTA's own capsule only shoved cars.
        n::set_entity_collision(ped, false, false);
        let poser = if config.ped_pose {
            Poser::new(ped, config.pose_mode)
                .map(|mut poser| {
                    // Skate 3's spawn pose stands square like the ped at rest.
                    let heading = coords::heading_degrees(n::get_entity_forward_vector(ped));
                    let view = ride.view();
                    let axes = model_axes(&view, ride.game.bone_names(), heading);
                    let calibrated = poser.rig.calibrate(|name| axes.get(name).copied());
                    log(&format!("Ped pose: {calibrated} spine/neck/head bones follow Skate 3 rotations"));
                    poser
                })
                .inspect_err(|e| {
                    log(&format!("Ped pose unavailable: {e}"));
                    n::notify(&format!("SkateGTA: ped pose unavailable ({e}), showing the skeleton"));
                })
                .ok()
        } else {
            None
        };
        if poser.is_none() {
            n::set_entity_visible(ped, false);
        }
        let draw_body = config.debug_body || poser.is_none() || prop.is_none();
        let cam = config.skate_camera.then(|| {
            let cam = n::create_cam();
            n::set_cam_active(cam, true);
            n::render_script_cams(true);
            cam
        });
        if let Some(error) = &ride.hud_error {
            log(&format!("Skate 3 HUD unavailable: {error}"));
        }
        let mut session = Self {
            ride,
            ped,
            prop,
            probe,
            cam,
            poser,
            draw_body,
            frames: 0,
            entities: Default::default(),
            collide_entities: config.collide_entities,
        };
        session.present(config);
        n::notify(&format!("SkateGTA {BUILD}: Skate 3 on"));
        Ok(session)
    }

    fn update(&mut self, config: &Config) -> Result<(), String> {
        if !n::does_entity_exist(self.ped) || n::is_entity_dead(self.ped) {
            return Err(String::new());
        }
        n::disable_all_control_actions(0);
        if n::is_disabled_control_just_pressed_in(FRONTEND, LEAVE) {
            return Err(String::new());
        }
        let elapsed = std::time::Duration::from_secs_f32(n::get_frame_time().clamp(0.0, 0.25));
        let pad = read_pad();
        let peds = if self.collide_entities {
            let (boxes, peds) = self.entities.gather(self.ped, self.prop, self.ride.hips_position());
            self.ride.set_entities(boxes);
            peds
        } else {
            Vec::new()
        };
        self.ride.refresh_world(&mut self.probe)?;
        let had_hud = self.ride.hud.is_some();
        self.ride
            .advance_game(elapsed, 4, pad)
            .map_err(|e| format!("gameplay stopped: {e}"))?;
        if !peds.is_empty() {
            let velocity = self.ride.deck_axes().forward.scale(self.ride.game.board_speed());
            let skater = [self.ride.deck_position(), self.ride.hips_position()];
            let knocked = self.entities.knock(&peds, &skater, velocity);
            if knocked > 0 {
                log(&format!("Knocked over {knocked} pedestrian(s) ({} this ride)", self.entities.knocked_total));
            }
        }
        if let Some(error) = self.ride.world_error.take() {
            log(&format!("Ground sample skipped: {error}"));
        }
        if had_hud && self.ride.hud.is_none() {
            log(self.ride.hud_error.as_deref().unwrap_or("Skate 3 HUD stopped"));
        }
        self.present(config);
        Ok(())
    }

    fn present(&mut self, config: &Config) {
        let view = self.ride.view();
        if let Some(prop) = self.prop {
            let q = coords::quaternion_mul(coords::quaternion(&view.axes), coords::yaw_quaternion(config.model_yaw_offset));
            n::set_entity_coords_no_offset(prop, view.deck.add(view.axes.up.scale(config.model_z_offset)));
            n::set_entity_quaternion(prop, q);
        }
        let origin = view.hips.add(GtaVec::new(0.0, 0.0, config.ped_z_offset - 1.0));
        // The skater stands sideways on the deck; the retarget turns the hips,
        // so the ped's own frame follows the board.
        let heading = coords::heading_degrees(view.axes.forward);
        n::set_entity_coords_no_offset(self.ped, origin);
        n::set_entity_heading(self.ped, heading);
        n::set_entity_yaw(self.ped, heading);
        // Pose relative to the ped's real matrix: GTA may not turn it exactly
        // (or at all) as asked, and the skeleton is drawn in that matrix.
        let actual = coords::heading_degrees(n::get_entity_forward_vector(self.ped));
        let actual_origin = n::get_entity_coords(self.ped);
        self.frames += 1;
        let off = ((actual - heading + 540.0) % 360.0) - 180.0;
        if self.frames % 120 == 1 || (off.abs() > 2.0 && self.frames % 30 == 1) {
            log(&format!(
                "Ped heading asked {heading:.1}, actual {actual:.1} (off {off:.1}), position off {:.3} m",
                length(actual_origin.sub(origin))
            ));
        }
        if let Some(poser) = &self.poser {
            n::set_ped_procedural_layers(self.ped, false);
            let model_heading = actual + config.ped_yaw_offset;
            if let Err(e) = poser.apply(self.ped, &view, self.ride.game.bone_names(), actual_origin, model_heading) {
                watch::stop();
                log(&format!("Ped pose stopped: {e}"));
                self.poser = None;
                self.draw_body = true;
                n::set_entity_visible(self.ped, false);
            }
        }
        if let (Some(cam), Some(camera)) = (self.cam, view.camera) {
            n::set_cam_coord(cam, camera.position);
            n::point_cam_at_coord(cam, camera.position.add(camera.forward.scale(10.0)));
            n::set_cam_fov(cam, camera.fov_degrees.clamp(10.0, 120.0));
        }
        if self.draw_body {
            self.draw_body(&view, config.debug_body || self.poser.is_none());
        }
        if config.debug_draw {
            self.draw_debug();
        }
        n::draw_text(
            &format!(
                "SkateGTA  {:?}  {:.1} km/h  [F5 / Back: off]",
                self.ride.game.state(),
                self.ride.game.board_speed() * 3.6
            ),
            0.01,
            0.01,
        );
        if config.hud {
            hud::with_painter(|p| p.draw(self.ride.hud.as_ref(), self.ride.hud_sprites()));
        }
    }

    /// The Skate 3 skater and board as lines, in place of GTA models.
    fn draw_body(&self, view: &crate::ride::View, skeleton: bool) {
        const BONE: [u8; 4] = [255, 255, 255, 255];
        const BOARD: [u8; 4] = [255, 170, 0, 255];
        let names = self.ride.game.bone_names();
        let body = |i: usize| {
            let name = names[i].as_str();
            name != "TRAJECTORY" && !name.ends_with("_REPARENTED") && !name.contains("WHEEL")
                && !name.starts_with("TRUCK") && name != "SKATEBOARD_ROOT"
        };
        for (i, &(p, parent)) in view.bones.iter().enumerate() {
            if !skeleton || parent < 0 || !body(i) || !body(parent as usize) {
                continue;
            }
            n::draw_line(p, view.bones[parent as usize].0, BONE);
        }
        let axes = view.axes;
        let [width, length] = self.ride.game.deck_size();
        let corner = |side: f32, end: f32| view.deck.add(axes.right.scale(side * width * 0.5)).add(axes.forward.scale(end * length * 0.5));
        let outline = [corner(-1.0, -0.8), corner(-1.0, 0.8), corner(-0.5, 1.0), corner(0.5, 1.0),
            corner(1.0, 0.8), corner(1.0, -0.8), corner(0.5, -1.0), corner(-0.5, -1.0)];
        for i in 0..outline.len() {
            n::draw_line(outline[i], outline[(i + 1) % outline.len()], BOARD);
        }
        let w = view.wheels;
        n::draw_line(w[BodyId::RightFrontWheel.index()], w[BodyId::LeftFrontWheel.index()], BOARD);
        n::draw_line(w[BodyId::RightBackWheel.index()], w[BodyId::LeftBackWheel.index()], BOARD);
        for wheel in w {
            let r = 0.03;
            n::draw_line(wheel.add(axes.forward.scale(-r)), wheel.add(axes.forward.scale(r)), BOARD);
            n::draw_line(wheel.add(axes.up.scale(-r)), wheel.add(axes.up.scale(r)), BOARD);
        }
    }

    fn draw_debug(&self) {
        for o in self.ride.obstacles() {
            let c = o.corners();
            for (a, b) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
                n::draw_line(c[a], c[b], [255, 80, 80, 160]);
            }
        }
        for line in self.ride.grind_lines() {
            for w in line.windows(2) {
                let lift = GtaVec::new(0.0, 0.0, 0.02);
                n::draw_line(w[0].add(lift), w[1].add(lift), [255, 220, 0, 255]);
            }
        }
        for line in self.ride.grind_lines() {
            for w in line.windows(2) {
                let lift = GtaVec::new(0.0, 0.0, 0.02);
                n::draw_line(w[0].add(lift), w[1].add(lift), [255, 220, 0, 255]);
            }
        }
        for patch in &self.ride.patches.patches {
            for t in &patch.triangles {
                let v = t.triangle.vertices.map(|p| self.ride.frame.to_gta(p));
                for i in 0..3 {
                    n::draw_line(v[i], v[(i + 1) % 3], [80, 255, 80, 90]);
                }
            }
        }
    }

    fn end(self) {
        watch::stop();
        n::set_ped_procedural_layers(self.ped, true);
        if let Some(cam) = self.cam {
            n::render_script_cams(false);
            n::destroy_cam(cam);
        }
        if let Some(prop) = self.prop {
            n::delete_entity(prop);
        }
        n::freeze_entity_position(self.ped, false);
        n::set_entity_has_gravity(self.ped, true);
        n::set_ped_gravity(self.ped, true);
        n::set_entity_collision(self.ped, true, true);
        n::set_ped_can_ragdoll(self.ped, true);
        n::set_entity_visible(self.ped, true);
        n::set_entity_coords(self.ped, self.ride.hips_position());
        hud::with_painter(|p| p.clear());
        n::notify("SkateGTA: Skate 3 off");
    }
}

fn spawn_prop(config: &Config, at: GtaVec) -> Option<n::Entity> {
    let model = crate::hash::joaat(&config.board_model);
    if !n::is_model_in_cdimage(model) {
        log(&format!("Board model {} is not in the game files", config.board_model));
        return None;
    }
    n::request_model(model);
    for _ in 0..200 {
        if n::has_model_loaded(model) {
            break;
        }
        shv::wait(10);
    }
    if !n::has_model_loaded(model) {
        log(&format!("Board model {} did not load", config.board_model));
        return None;
    }
    let prop = n::create_object(model, at);
    n::set_model_as_no_longer_needed(model);
    if prop == 0 {
        return None;
    }
    n::set_entity_as_mission_entity(prop);
    n::set_entity_collision(prop, false, false);
    n::freeze_entity_position(prop, true);
    Some(prop)
}
