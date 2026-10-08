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
    deck_size: [f32; 2],
    physics: GamePhysics,
    skater: SkaterRuntime,
    controls: PlayerControls,
    graphs: StockGraphs,
    camera: CameraRuntime,
    accumulator: Duration,
    /// The audio-state record of the ticks since the audio host last ran.
    audio_cues: crate::game_audio::skate_events::Cues,
    audio_seen: crate::game_audio::skate_events::Seen,
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
        let data = skate_data::collections::Collections::load(root)?;
        let deck = |field| data.float("physicsdeck", "default", field);
        let deck_size = [deck("DeckWidth")?, deck("DeckMidLength")? + 2.0 * deck("DeckBackEndSize")?];
        Ok(Self { deck_size, physics, skater, controls, graphs, camera, accumulator: Duration::ZERO, audio_cues: Default::default(), audio_seen: Default::default() })
    }

    /// Static collision near the skater, in skate space (metres, Y up), with
    /// query metadata. Call before the first tick; the stock test course is
    /// only a placeholder until then.
    pub fn set_world(&mut self, world: BoardWorld) -> Result<(), String> {
        self.physics.replace_world(world)
    }

    /// Grind lines in skate space (polylines along ledge, curb and rail tops).
    /// Replacing them mid-grind would drop the rail under the board, so hosts
    /// should only call this while `grinding()` is false.
    pub fn set_grind_rails(&mut self, rails: &[Vec<skate_core::math::Vector3>]) -> Result<(), String> {
        let rails: Vec<skate_data::skate_map::Rail> = rails
            .iter()
            .enumerate()
            .filter(|(_, points)| points.len() >= 2)
            .map(|(i, points)| skate_data::skate_map::Rail {
                name: format!("host_edge_{i}"),
                points: points.iter().map(|p| [p.x, p.y, p.z]).collect(),
                closed: false,
                native: None,
            })
            .collect();
        let provider = self.physics.replace_grind_rails(&rails)?;
        self.skater.trajectory.bind_grind_world(provider);
        Ok(())
    }

    /// The skater is in one of the grind states.
    pub fn grinding(&self) -> bool {
        (400..=405).contains(&(self.state() as u32))
    }

    /// Line-query source beyond the static world (cleared by `set_world`).
    pub fn set_external_queries(
        &mut self,
        queries: Option<std::sync::Arc<dyn skate_core::physics::board_world::ExternalQueries>>,
    ) {
        self.physics.set_external_queries(queries);
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
        )?;
        let dt = self.tick_period().as_secs_f32();
        crate::game_audio::skate_events::observe(&self.physics, &mut self.skater, &mut self.audio_cues, &mut self.audio_seen, dt);
        Ok(())
    }

    /// One rendered frame of the skater's Skate 3 sounds, on the ticks run since the
    /// last call. `camera` in skate space (None: the Skate 3 camera); `dt` the frame's
    /// seconds; `overstep` the fixed step's overstep fraction (the rendered wheels).
    pub fn audio_frame(&mut self, audio: &mut crate::game_audio::GameAudio, camera: Option<crate::game_audio::Camera>, dt: f32, overstep: f32) {
        let camera = camera.or_else(|| {
            self.camera_frame().map(|c| { let (p, f) = (c.position, c.basis.columns[2]); crate::game_audio::Camera { position: [p[0], p[1], p[2]], forward: [f[0], f[1], f[2]] } })
        });
        audio.frame(&mut self.audio_cues, camera, dt, overstep.clamp(0.0, 1.0));
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

    /// Every animated bone in skate world space with its parent index (-1 for
    /// roots), from the completed render pose.
    pub fn skeleton_world(&self) -> Vec<(skate_core::math::Vector3, i32)> {
        let root = self.skater.animated_skeleton.roots.animation_to_world;
        let parents = &self.skater.animation.evaluator.frames.parents;
        self.skater
            .render_pose
            .iter()
            .zip(parents)
            .map(|(m, &parent)| {
                let p = m[3];
                let world = core::array::from_fn::<f32, 3, _>(|r| {
                    root[0][r] * p[0] + root[1][r] * p[1] + root[2][r] * p[2] + root[3][r]
                });
                (skate_core::math::Vector3::new(world[0], world[1], world[2]), parent)
            })
            .collect()
    }

    /// Every animated bone's axes (columns) in skate world space, same order
    /// as `skeleton_world`.
    pub fn skeleton_world_axes(&self) -> Vec<[skate_core::math::Vector3; 3]> {
        let root = self.skater.animated_skeleton.roots.animation_to_world;
        self.skater
            .render_pose
            .iter()
            .map(|m| {
                core::array::from_fn(|k| {
                    let a = m[k];
                    let w = core::array::from_fn::<f32, 3, _>(|r| root[0][r] * a[0] + root[1][r] * a[1] + root[2][r] * a[2]);
                    skate_core::math::Vector3::new(w[0], w[1], w[2])
                })
            })
            .collect()
    }

    /// Stock deck width and overall length (physicsdeck), metres.
    pub fn deck_size(&self) -> [f32; 2] {
        self.deck_size
    }

    pub fn bone_names(&self) -> &[String] {
        &self.skater.animation.evaluator.frames.bone_names
    }

    /// The Skate 3 gameplay camera for this tick, in skate space.
    pub fn camera_frame(&self) -> Option<skate_core::camera::CameraFrame> {
        self.camera.presentation_frame()
    }

    /// Retail Camera Angle setting: true selects the low (classic) camera graph.
    pub fn set_low_camera(&mut self, low: bool) {
        let angle = if low { crate::camera::CameraAngle::Low } else { crate::camera::CameraAngle::High };
        self.camera.set_camera_type(angle.graph_type());
    }

    pub(crate) fn scoring(&self) -> &crate::scoring_runtime::Runtime {
        &self.skater.scoring
    }

    /// Name of the trick the scoring currently shows (changes as it is modified).
    pub fn trick_name(&self) -> &str {
        self.skater.scoring.trick_name()
    }

    /// The trick the scoring announced on the last tick, if any.
    pub fn announced_trick(&self) -> Option<&str> {
        self.skater.scoring.new_trick.then(|| self.skater.scoring.trick_name())
    }

    /// Name of the stock camera shot the Skate 3 camera graph selected.
    pub fn camera_shot(&self) -> String {
        self.camera.selected_shot().to_string()
    }

    pub fn wheel_contacts(&self) -> usize {
        self.physics.riding.ground.wheel_contact_count as usize
    }
}
