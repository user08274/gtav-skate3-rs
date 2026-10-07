//! ScriptHookV script: DllMain registration, the script fiber loop and one
//! riding session (board prop, attached player, fixed-step simulation).
mod natives;
mod shv;

use crate::{
    assets,
    config::{self, Config},
    coords::{self, GtaVec},
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

static MODULE: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

#[unsafe(no_mangle)]
pub extern "system" fn DllMain(module: HMODULE, reason: u32, _reserved: *mut core::ffi::c_void) -> i32 {
    match reason {
        DLL_PROCESS_ATTACH => {
            MODULE.store(module, Ordering::SeqCst);
            if let Err(error) = shv::register(module, script_main) {
                log(&format!("Cannot register with ScriptHookV: {error}"));
                return 0;
            }
        }
        DLL_PROCESS_DETACH => shv::unregister(module),
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

fn run() {
    let config = load_config();
    log("SkateGTA loaded");
    let mut data: Option<Collections> = None;
    let mut session: Option<Session> = None;
    let mut key_was_down = false;
    loop {
        let key_down = unsafe { GetAsyncKeyState(config.toggle_key as i32) } as u16 & 0x8000 != 0;
        let toggled = key_down && !key_was_down;
        key_was_down = key_down;
        if toggled {
            match session.take() {
                Some(s) => s.end(),
                None => match load_data(&config, &mut data).and_then(|d| Session::start(&config, d)) {
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
