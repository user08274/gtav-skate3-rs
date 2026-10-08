//! The complete Skate 3 gameplay (skate-gameplay) placed in the GTA world:
//! a skate-space frame at the player's feet, ground patches that follow the
//! board and the skater, and GTA-space views of the results.
use crate::{
    coords::{EntityAxes, Frame, GtaVec},
    edges::{EdgeFinder, EdgeSettings},
    obstacles::{Obstacle, WallFinder, WallSettings},
    far::{FarField, FarSettings},
    terrain::{GroundProbe, PatchSettings, Patches},
};
use skate_core::{math::Basis3, physics::contact::RetailContactMaterial};
use skate_gameplay::{
    host::{Game, Mode, SPAWN_GROUND_HEIGHT},
    hud::{Hud, Sprite},
};
use std::{path::Path, time::Duration};

/// The original host's static floor material: zero friction and full
/// restitution, so the moving volume's own material decides each contact.
const FLOOR: RetailContactMaterial = RetailContactMaterial {
    static_friction: 0.0,
    dynamic_friction: 0.0,
    restitution: 1.0,
};

/// Changes when any obstacle moves by a centimetre or more.
fn fingerprint(obstacles: &[Obstacle]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    let mut eat = |v: f32| {
        h = (h ^ (v * 100.0).round() as i64 as u64).wrapping_mul(0x0100_0000_01b3);
    };
    for o in obstacles {
        for v in [o.center.x, o.center.y, o.center.z] {
            eat(v);
        }
        for a in o.axes {
            for v in [a.x, a.y, a.z] {
                eat(v);
            }
        }
        for v in o.half {
            eat(v);
        }
    }
    h
}

pub struct Ride {
    /// Boxed: the gameplay state is far larger than a script fiber's stack.
    pub game: Box<Game>,
    accumulator: Duration,
    previous: View,
    current: View,
    pub frame: Frame,
    pub patches: Patches,
    pub far: FarField,
    /// The original scoring HUD; None when its files are missing or it failed.
    pub hud: Option<Hud>,
    pub hud_error: Option<String>,
    hud_sprites: Vec<Sprite>,
    /// Grindable step edges around the skater and the lines last given to the game.
    pub edges: EdgeFinder,
    /// Last ground the game refused (logged by the host; the old ground stays).
    pub world_error: Option<String>,
    /// Walls from sideways rays, and entity boxes the host sets each frame.
    pub walls: WallFinder,
    pub find_walls: bool,
    entities: Vec<Obstacle>,
    /// Obstacles in the world the game has, and their fingerprint.
    obstacles: Vec<Obstacle>,
    obstacles_key: u64,
    /// Look for grindable edges at all (`GrindEdges`).
    pub grind_edges: bool,
    edges_given: u64,
    grind_lines: Vec<Vec<GtaVec>>,
}

#[derive(Clone, Copy, Debug)]
pub struct CameraView {
    pub position: GtaVec,
    pub forward: GtaVec,
    pub fov_degrees: f32,
}

impl Ride {
    /// `ground` is the surface under the player; the board faces `heading`.
    pub fn start(
        root: &Path,
        mode: Mode,
        ground: GtaVec,
        heading_degrees: f32,
        patch: PatchSettings,
        probe: &mut dyn GroundProbe,
    ) -> Result<Self, String> {
        let game = crate::bigstack::run(|| Game::load(root, mode).map(Box::new))?;
        let (hud, hud_error) = match crate::bigstack::run(|| Hud::load(root, &game)) {
            Ok(hud) => (Some(hud), None),
            Err(error) => (None, Some(error)),
        };
        let frame = Frame::facing(ground.add(GtaVec::new(0.0, 0.0, -SPAWN_GROUND_HEIGHT)), heading_degrees);
        let view = View::capture(&game, &frame);
        let mut ride = Self {
            game,
            accumulator: Duration::ZERO,
            previous: view.clone(),
            current: view,
            frame,
            patches: Patches::new(patch, FLOOR),
            far: FarField::new(FarSettings::default()),
            hud,
            hud_error,
            hud_sprites: Vec::new(),
            edges: EdgeFinder::new(EdgeSettings::default()),
            grind_edges: true,
            world_error: None,
            walls: WallFinder::new(WallSettings::default()),
            find_walls: true,
            entities: Vec::new(),
            obstacles: Vec::new(),
            obstacles_key: 0,
            edges_given: 0,
            grind_lines: Vec::new(),
        };
        // Fill the whole far field once so the camera starts with full ground.
        let side = 2 * ride.far.settings.radius_cells as usize + 1;
        while ride.far.sample_count() < side * side {
            let before = ride.far.sample_count();
            ride.far.update(probe, ground);
            if ride.far.sample_count() == before {
                break;
            }
        }
        ride.refresh_world(probe)?;
        Ok(ride)
    }

    /// Probes GTA around the board and skater. Must run on the script fiber.
    pub fn refresh_world(&mut self, probe: &mut dyn GroundProbe) -> Result<(), String> {
        let centers = [self.deck_position(), self.hips_position()];
        // An empty sample (nothing under the board within reach) keeps the
        // last ground: the game cannot take a world without surfaces.
        let resampled = self.patches.refresh(probe, &self.frame, &centers);
        if self.find_walls {
            self.walls.update(probe, self.deck_position());
        }
        let mut obstacles = if self.find_walls { self.walls.obstacles() } else { Vec::new() };
        obstacles.extend(self.entities.iter().copied());
        // Only what the board or the body can reach soon matters to the physics.
        let (deck, hips) = (self.deck_position(), self.hips_position());
        obstacles.retain(|o| o.distance(deck).min(o.distance(hips)) < 5.0);
        let key = fingerprint(&obstacles);
        let found = (resampled || key != self.obstacles_key)
            && self.patches.patches.iter().any(|p| !p.triangles.is_empty());
        if found {
            let extra = crate::obstacles::triangles(&obstacles, &self.frame, FLOOR);
            self.obstacles = obstacles;
            self.obstacles_key = key;
            let world = crate::terrain::world_of(&self.patches.patches, &extra);
            let game = &mut self.game;
            if let Err(error) = crate::bigstack::run(move || game.set_world(world)) {
                self.world_error = Some(error);
            }
        }
        self.far.update(probe, self.hips_position());
        let queries = self.far.queries(self.frame, self.patches.rects());
        self.game.set_external_queries(Some(std::sync::Arc::new(queries)));
        self.refresh_grind_lines(probe)
    }

    /// Finds step edges near the skater and hands them to the game as grind
    /// lines. Only while rolling or walking: swapping the lines in the air or
    /// mid-grind would pull the rail out from under a landing or a grind.
    fn refresh_grind_lines(&mut self, probe: &mut dyn GroundProbe) -> Result<(), String> {
        if !self.grind_edges {
            return Ok(());
        }
        let center = self.hips_position();
        self.edges.update(probe, center);
        let settled = matches!(self.game.state() as u32, 100..=105 | 500 | 502);
        if self.edges.version == self.edges_given || !settled {
            return Ok(());
        }
        let lines = self.edges.polylines(center, self.edges.settings.radius + 2.0);
        let rails: Vec<Vec<skate_core::math::Vector3>> =
            lines.iter().map(|line| line.iter().map(|&p| self.frame.to_skate(p)).collect()).collect();
        let game = &mut self.game;
        crate::bigstack::run(move || game.set_grind_rails(&rails))?;
        self.grind_lines = lines;
        self.edges_given = self.edges.version;
        Ok(())
    }

    /// Entity boxes (vehicles, props, pedestrians) for the next world refresh.
    pub fn set_entities(&mut self, boxes: Vec<Obstacle>) {
        self.entities = boxes;
    }

    /// Obstacles in the game's current world, in GTA space.
    pub fn obstacles(&self) -> &[Obstacle] {
        &self.obstacles
    }

    /// The grind lines the game currently has, in GTA space.
    pub fn grind_lines(&self) -> &[Vec<GtaVec>] {
        &self.grind_lines
    }

    pub fn advance(
        &mut self,
        elapsed: Duration,
        max_ticks: u32,
        pad: [f32; 18],
        probe: &mut dyn GroundProbe,
    ) -> Result<u32, String> {
        self.refresh_world(probe)?;
        self.advance_game(elapsed, max_ticks, pad)
    }

    /// The gameplay ticks alone, on a large-stack thread. Calls no natives.
    /// Keeps the last two tick snapshots for interpolated presentation.
    pub fn advance_game(&mut self, elapsed: Duration, max_ticks: u32, pad: [f32; 18]) -> Result<u32, String> {
        let period = self.game.tick_period();
        self.accumulator = (self.accumulator + elapsed).min(period * max_ticks);
        let due = (self.accumulator.as_nanos() / period.as_nanos().max(1)) as u32;
        self.accumulator -= period * due;
        if due == 0 {
            return Ok(0);
        }
        let (game, frame, hud) = (&mut self.game, &self.frame, &mut self.hud);
        let (second_last, last, sprites) = crate::bigstack::run(move || {
            let mut second_last = None;
            let mut last = None;
            let mut hud_error = None;
            for _ in 0..due {
                game.tick(pad)?;
                if let Some(h) = hud.as_mut() {
                    if let Err(error) = h.update(game) {
                        hud_error = Some(error);
                        *hud = None;
                    }
                }
                second_last = last.take();
                last = Some(View::capture(game, frame));
            }
            let sprites = match (hud.as_ref().map(|h| h.sprites()), hud_error) {
                (_, Some(error)) | (Some(Err(error)), None) => Err(error),
                (Some(Ok(sprites)), None) => Ok(sprites),
                (None, None) => Ok(Vec::new()),
            };
            Ok((second_last, last.expect("at least one tick ran"), sprites))
        })?;
        match sprites {
            Ok(sprites) => self.hud_sprites = sprites,
            Err(error) => {
                self.hud = None;
                self.hud_sprites.clear();
                self.hud_error = Some(format!("Skate 3 HUD stopped: {error}"));
            }
        }
        self.previous = match second_last {
            Some(view) => view,
            None => std::mem::replace(&mut self.current, last.clone()),
        };
        self.current = last;
        Ok(due)
    }

    /// One rendered frame of the Skate 3 sounds (after [`Ride::advance`]). `camera`: the
    /// game camera's position and forward in GTA space (None: the Skate 3 camera).
    pub fn audio_frame(&mut self, audio: &mut skate_gameplay::game_audio::GameAudio, camera: Option<(GtaVec, GtaVec)>, dt: f32) {
        let camera = camera.map(|(p, f)| {
            let (p, f) = (self.frame.to_skate(p), self.frame.dir_to_skate(f));
            skate_gameplay::game_audio::Camera { position: [p.x, p.y, p.z], forward: [f.x, f.y, f.z] }
        });
        let overstep = self.accumulator.as_secs_f32() / self.game.tick_period().as_secs_f32().max(1e-6);
        let game = &mut self.game;
        crate::bigstack::run(move || {
            game.audio_frame(audio, camera, dt, overstep);
            Ok::<(), String>(())
        })
        .ok();
    }

    /// The HUD as of the last tick (it is not interpolated).
    pub fn hud_sprites(&self) -> &[Sprite] {
        &self.hud_sprites
    }

    /// Presentation between the last two ticks, by the unspent frame time.
    pub fn view(&self) -> View {
        let period = self.game.tick_period().as_secs_f32();
        let t = if period > 0.0 { (self.accumulator.as_secs_f32() / period).clamp(0.0, 1.0) } else { 1.0 };
        View::blend(&self.previous, &self.current, t)
    }

    pub fn deck_position(&self) -> GtaVec {
        self.frame.to_gta(self.game.deck().translation)
    }

    pub fn deck_axes(&self) -> EntityAxes {
        self.frame.entity_axes(self.game.deck().basis)
    }

    pub fn hips_position(&self) -> GtaVec {
        self.frame.to_gta(self.game.hips_position())
    }

    /// Facing of the animated skater root (its native `At` column).
    pub fn skater_forward(&self) -> GtaVec {
        let m = self.game.skater_root();
        let at = skate_core::math::Vector3::new(m[2][0], m[2][1], m[2][2]);
        self.frame.dir_to_gta(at)
    }

    pub fn camera(&self) -> Option<CameraView> {
        let frame = self.game.camera_frame()?;
        let p = frame.position;
        let at = frame.basis.columns[2];
        Some(CameraView {
            position: self.frame.to_gta(skate_core::math::Vector3::new(p[0], p[1], p[2])),
            forward: self.frame.dir_to_gta(skate_core::math::Vector3::new(at[0], at[1], at[2])),
            fov_degrees: frame.field_of_view_degrees,
        })
    }

    pub fn skate_basis_axes(&self, basis: Basis3) -> EntityAxes {
        self.frame.entity_axes(basis)
    }
}

/// One tick's presentation state in GTA space.
#[derive(Clone, Debug)]
pub struct View {
    pub deck: GtaVec,
    pub axes: EntityAxes,
    pub wheels: [GtaVec; 4],
    /// World bone positions and parent indices (`Game::bone_names` order).
    pub bones: Vec<(GtaVec, i32)>,
    /// World axes of the same bones (GTA space, Skate 3's bone frames).
    pub bone_axes: Vec<[GtaVec; 3]>,
    pub hips: GtaVec,
    pub forward: GtaVec,
    pub camera: Option<CameraView>,
}

impl View {
    fn capture(game: &Game, frame: &Frame) -> Self {
        let deck = game.deck();
        let parts = game.board_parts();
        let root = game.skater_root();
        let at = skate_core::math::Vector3::new(root[2][0], root[2][1], root[2][2]);
        Self {
            deck: frame.to_gta(deck.translation),
            axes: frame.entity_axes(deck.basis),
            wheels: core::array::from_fn(|i| frame.to_gta(parts[i].translation)),
            bones: game.skeleton_world().into_iter().map(|(p, parent)| (frame.to_gta(p), parent)).collect(),
            bone_axes: game.skeleton_world_axes().into_iter().map(|axes| axes.map(|a| frame.dir_to_gta(a))).collect(),
            hips: frame.to_gta(game.hips_position()),
            forward: frame.dir_to_gta(at),
            camera: game.camera_frame().map(|c| {
                let p = c.position;
                let f = c.basis.columns[2];
                CameraView {
                    position: frame.to_gta(skate_core::math::Vector3::new(p[0], p[1], p[2])),
                    forward: frame.dir_to_gta(skate_core::math::Vector3::new(f[0], f[1], f[2])),
                    fov_degrees: c.field_of_view_degrees,
                }
            }),
        }
    }

    pub fn blend(a: &Self, b: &Self, t: f32) -> Self {
        let axes = EntityAxes {
            right: a.axes.right.lerp(b.axes.right, t).normalized(),
            forward: a.axes.forward.lerp(b.axes.forward, t).normalized(),
            up: a.axes.up.lerp(b.axes.up, t).normalized(),
        };
        let bones = if a.bones.len() == b.bones.len() {
            a.bones.iter().zip(&b.bones).map(|(x, y)| (x.0.lerp(y.0, t), y.1)).collect()
        } else {
            b.bones.clone()
        };
        let bone_axes = if a.bone_axes.len() == b.bone_axes.len() {
            a.bone_axes
                .iter()
                .zip(&b.bone_axes)
                .map(|(x, y)| std::array::from_fn(|k| x[k].lerp(y[k], t).normalized()))
                .collect()
        } else {
            b.bone_axes.clone()
        };
        let camera = match (a.camera, b.camera) {
            (Some(x), Some(y)) => Some(CameraView {
                position: x.position.lerp(y.position, t),
                forward: x.forward.lerp(y.forward, t).normalized(),
                fov_degrees: x.fov_degrees + (y.fov_degrees - x.fov_degrees) * t,
            }),
            (_, y) => y,
        };
        Self {
            deck: a.deck.lerp(b.deck, t),
            axes,
            wheels: core::array::from_fn(|i| a.wheels[i].lerp(b.wheels[i], t)),
            bones,
            bone_axes,
            hips: a.hips.lerp(b.hips, t),
            forward: a.forward.lerp(b.forward, t).normalized(),
            camera,
        }
    }
}
