//! Embedding API: one skater with the complete Skate 3 gameplay pipeline
//! (controller graphs, physical states, shared board/skater solve, camera),
//! loaded from the player's converted files, in a world the host supplies.
use crate::{
    camera::CameraRuntime,
    difficulty::Difficulty,
    graph_runtime::StockGraphs,
    physics::{GamePhysics, PlayerControls, SkaterRuntime},
};
use skate_core::{
    input::{gameplay_map::GameplayActions, tick::TickInput},
    physics::{
        board::BodyId,
        board_world::BoardWorld,
        drive_frames::RetailAffineTransform,
    },
    player::state::PhysicalStateId,
};
use skate_data::GameAssets;
use std::{path::Path, time::Duration};

pub use skate_core::input::gameplay_map::ACTIONS;

/// Physics mode the stock `physics_mode` collections define.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Easy,
    Normal,
    Hardcore,
}

impl Mode {
    fn difficulty(self) -> Difficulty {
        match self {
            Self::Easy => Difficulty::Easy,
            Self::Normal => Difficulty::Normal,
            Self::Hardcore => Difficulty::Hardcore,
        }
    }
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "easy" => Some(Self::Easy),
            "normal" => Some(Self::Normal),
            "hardcore" => Some(Self::Hardcore),
            _ => None,
        }
    }
}

/// Files the gameplay pipeline reads, relative to the converted assets root.
pub const REQUIRED_FILES: [&str; 16] = [
    "private/stock/skater-collections.json",
    "private/stock/physics-skeletons.json",
    "private/stock/data/anim/OnBoard.abin",
    "private/stock/data/anim/OffBoard.abin",
    "private/stock/data/state/ActionGraph_OnBoard.stategraph",
    "private/stock/data/state/MotionGraph_OnBoard.stategraph",
    "private/stock/data/script/camera/Default_cameragraph.stategraph",
    "private/stock/data/camera/1.shk",
    "private/stock/data/camera/2.shk",
    "private/stock/data/joystick/skater.pat",
    "private/stock/data/joystick/skater90.pat",
    "private/stock/data/joystick/skaterN90.pat",
    "private/stock/data/joystick/skater_air.pat",
    "private/stock/data/joystick/skater_fingerflip.pat",
    "private/stock/data/joystick/skaterls.pat",
    "private/stock/data/joystick/skaterstep.pat",
];

pub fn missing_files(root: &Path) -> Vec<&'static str> {
    REQUIRED_FILES.iter().copied().filter(|f| !root.join(f).is_file()).collect()
}

/// HIPS in the stock PHYS_TPOSE physics skeleton (SKATEBOARD_ROOT, NECK1, NECK,
/// LEFTHAND, ... HIPS); the hand drives index 3 and 7 the same way.
const HIPS_PART: usize = 23;

pub struct Game {
    physics: GamePhysics,
    skater: SkaterRuntime,
    controls: PlayerControls,
    graphs: StockGraphs,
    camera: CameraRuntime,
    accumulator: Duration,
}

/// Where the original test course's spawn surface sits in skate space. Hosts
/// place their ground there so the stock spawn lands on it without dropping.
pub const SPAWN_GROUND_HEIGHT: f32 = -0.035;

impl Game {
    pub fn load(root: &Path, mode: Mode) -> Result<Self, String> {
        let missing = missing_files(root);
        if !missing.is_empty() {
            return Err(format!("missing converted Skate 3 files: {}", missing.join(", ")));
        }
        let manifest = GameAssets::parse(
            r#"{"version":1,"character_scene":"private/skater.glb","initial_animation":"R_IDLE_HCOM_000",
            "action_graph":"private/stock/data/state/ActionGraph_OnBoard.stategraph",
            "motion_graph":"private/stock/data/state/MotionGraph_OnBoard.stategraph"}"#,
        )
        .map_err(|e| e.to_string())?;
        let graphs = StockGraphs::load(root, &manifest)?;
        let difficulty = mode.difficulty();
        let physics = GamePhysics::load_with_difficulty(root, None, difficulty)?;
        let skater = SkaterRuntime::load(root, &graphs, &physics, difficulty.profile_key())?;
        let controls = PlayerControls::load(root)?;
        let camera = CameraRuntime::load(root)?;
        Ok(Self { physics, skater, controls, graphs, camera, accumulator: Duration::ZERO })
    }

    /// Static collision near the skater, in skate space (metres, Y up), with
    /// query metadata. Call before the first tick; the stock test course is
    /// only a placeholder until then.
    pub fn set_world(&mut self, world: BoardWorld) -> Result<(), String> {
        self.physics.replace_world(world)
    }

    pub fn tick_period(&self) -> Duration {
        self.physics.clock_period()
    }

    /// Runs the fixed ticks `elapsed` covers, at most `max_ticks`, all with the
    /// same sampled pad state (`ACTIONS` order).
    pub fn advance(&mut self, elapsed: Duration, max_ticks: u32, pad: [f32; 18]) -> Result<u32, String> {
        let period = self.tick_period();
        self.accumulator = (self.accumulator + elapsed).min(period * max_ticks);
        let mut ran = 0;
        while self.accumulator >= period {
            self.accumulator -= period;
            self.tick(pad)?;
            ran += 1;
        }
        Ok(ran)
    }

    pub fn tick(&mut self, pad: [f32; 18]) -> Result<(), String> {
        let mut sampled = GameplayActions::from_values(pad);
        self.controls.update_for_physics(&mut sampled, &self.physics, &self.skater, &self.camera)?;
        self.controls.publish_gestures(
            self.physics.difficulty_index(),
            self.skater.player_input.physical.state.state_16,
        );
        let input = TickInput::new(self.physics.ticks, GameplayActions::from_values(pad), true);
        crate::physics::advance(
            &mut self.physics,
            &mut self.skater,
            &mut self.controls,
            &self.graphs,
            &input,
            &mut self.camera,
        )
    }

    pub fn ticks(&self) -> u64 {
        self.physics.ticks
    }

    pub fn state(&self) -> PhysicalStateId {
        self.skater.player_state.current()
    }

    pub fn board_parts(&self) -> [RetailAffineTransform; 7] {
        self.physics.board.part_transforms()
    }

    pub fn deck(&self) -> RetailAffineTransform {
        self.board_parts()[BodyId::Deck.index()]
    }

    pub fn board_speed(&self) -> f32 {
        let v = self.physics.board.bodies()[BodyId::Deck.index()].rates.linear_velocity;
        (v.x * v.x + v.y * v.y + v.z * v.z).sqrt()
    }

    /// Physical skater bodies, in PHYS_TPOSE bone order (0 is SKATEBOARD_ROOT).
    pub fn skater_body_positions(&self) -> Vec<skate_core::math::Vector3> {
        self.skater.skeleton.bodies().iter().map(|b| b.rates.position).collect()
    }

    pub fn hips_position(&self) -> skate_core::math::Vector3 {
        self.skater.skeleton.bodies()[HIPS_PART].rates.position
    }

    /// Animation root to world, native column-major 4x4 (skate space).
    pub fn skater_root(&self) -> [[f32; 4]; 4] {
        self.skater.animated_skeleton.roots.animation_to_world
    }

    /// Completed skater pose for rendering, one native matrix per bone.
    pub fn render_pose(&self) -> &[skate_core::animation::output::NativeMatrix] {
        &self.skater.render_pose
    }

    pub fn bone_names(&self) -> &[String] {
        &self.skater.animation.evaluator.frames.bone_names
    }

    /// The Skate 3 gameplay camera for this tick, in skate space.
    pub fn camera_frame(&self) -> Option<skate_core::camera::CameraFrame> {
        self.camera.presentation_frame()
    }

    pub fn wheel_contacts(&self) -> usize {
        self.physics.riding.ground.wheel_contact_count as usize
    }
}
