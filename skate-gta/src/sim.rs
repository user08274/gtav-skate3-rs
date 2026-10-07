//! Fixed-step board simulation: the recovered Skate 3 board assembly and
//! contact solver, fed by GTA collision. All physical data is loaded from the
//! player's converted Skate 3 files; nothing here substitutes stock values.
use crate::{
    colliders,
    coords::{Frame, GtaVec, spawn_transform},
    settings::PhysicsSettings,
    terrain::{self, GroundProbe, Patch, PatchSettings},
};
use skate_core::{
    math::Vector3,
    physics::{
        board::BodyId,
        board_runtime::{BoardMotion, BoardRuntime},
        board_world::{BoardWorld, ContactRetentionSettings},
        collision::WorldContactSettings,
        drive_frames::RetailAffineTransform,
        force_queue::QueuedPointForce,
    },
};
use skate_data::collections::Collections;

/// Stock values the phase-1 test controls read from the player's data.
#[derive(Clone, Copy, Debug)]
pub struct StockControlData {
    pub maximum_pushable_speed: f32,
    pub foot_brake_force: f32,
    pub slow_speed: f32,
}

impl StockControlData {
    pub fn load(data: &Collections) -> Result<Self, String> {
        let f = |class, field| data.float(class, "default", field);
        Ok(Self {
            maximum_pushable_speed: f("physics_push", "MaxPushableSpeed")?,
            foot_brake_force: f("physics_brakes", "FootBrakeForce")?,
            slow_speed: f("physics_brakes", "SlowSpeed")?,
        })
    }
}

/// Temporary rider stand-in until the stock ground runtime is ported.
#[derive(Clone, Copy, Debug)]
pub struct TestControlTuning {
    pub push_force: f32,
    pub steer_angle: f32,
    pub invert_steer: bool,
}

impl Default for TestControlTuning {
    fn default() -> Self {
        Self {
            push_force: 40.0,
            steer_angle: 0.3,
            invert_steer: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Controls {
    /// -1 = full left, +1 = full right.
    pub steer: f32,
    pub push: bool,
    pub brake: bool,
}

pub struct BoardSim {
    pub frame: Frame,
    pub board: BoardRuntime,
    pub settings: PhysicsSettings,
    stock: StockControlData,
    pub tuning: TestControlTuning,
    world: BoardWorld,
    patch: Option<Patch>,
    pub patch_settings: PatchSettings,
    query: WorldContactSettings,
    retention: ContactRetentionSettings,
    pub ticks: u64,
    pub contact_count: usize,
    accumulator: f32,
}

const SETTLE_TICKS: u32 = 30;

#[derive(Debug, PartialEq)]
pub enum StepError {
    NonFinite { tick: u64 },
}

impl BoardSim {
    /// `ground` is where the wheels should touch; the deck faces `heading`.
    pub fn spawn(
        data: &Collections,
        ground: GtaVec,
        heading_degrees: f32,
        tuning: TestControlTuning,
        patch_settings: PatchSettings,
    ) -> Result<Self, String> {
        let settings = PhysicsSettings::load(data)?;
        let stock = StockControlData::load(data)?;
        let frame = Frame::new(ground);
        let lift = settings.wheel_radius - settings.authored[0].translation.y;
        let spawn = spawn_transform(Vector3::new(0.0, lift, 0.0), heading_degrees);
        let board = BoardRuntime::new(
            settings.masses,
            settings.authored,
            spawn,
            settings.step.simulation,
            BoardMotion::Active,
        );
        Ok(Self {
            frame,
            board,
            settings,
            stock,
            tuning,
            world: BoardWorld::new(Vec::new()),
            patch: None,
            patch_settings,
            query: query_settings(),
            retention: retention_settings(),
            ticks: 0,
            contact_count: 0,
            accumulator: 0.0,
        })
    }

    /// The authored truck poses do not match the truck drive targets, so a
    /// freshly built assembly snaps on its first ticks. Let it settle in
    /// place, then discard the momentum that snap produced.
    pub fn settle(&mut self, probe: &mut dyn GroundProbe) -> Result<(), StepError> {
        for _ in 0..SETTLE_TICKS {
            self.tick(Controls::default(), probe)?;
            self.stop();
        }
        Ok(())
    }

    pub fn stop(&mut self) {
        for body in self.board.bodies_mut() {
            body.rates.linear_velocity = Vector3::ZERO;
            body.rates.angular_velocity = Vector3::ZERO;
        }
    }

    pub fn time_step(&self) -> f32 {
        self.settings.step.simulation.time_step
    }

    /// Run as many fixed ticks as `frame_time` covers (at most `max_ticks`).
    pub fn advance(
        &mut self,
        frame_time: f32,
        max_ticks: u32,
        controls: Controls,
        probe: &mut dyn GroundProbe,
    ) -> Result<u32, StepError> {
        let dt = self.time_step();
        self.accumulator = (self.accumulator + frame_time.max(0.0)).min(dt * max_ticks as f32);
        let mut ran = 0;
        while self.accumulator >= dt {
            self.accumulator -= dt;
            self.tick(controls, probe)?;
            ran += 1;
        }
        Ok(ran)
    }

    pub fn tick(&mut self, controls: Controls, probe: &mut dyn GroundProbe) -> Result<(), StepError> {
        self.refresh_patch(probe);
        self.board.clear_forces();
        self.apply_controls(controls);
        // A positive truck target leans the board into a left turn.
        let steer = if self.tuning.invert_steer { controls.steer } else { -controls.steer };
        let target = steer.clamp(-1.0, 1.0) * self.tuning.steer_angle;
        let volumes = colliders::world_volumes(&self.board, &self.settings);
        let contacts = self
            .world
            .query_primitives(&volumes, self.query, self.retention)
            .to_vec();
        self.contact_count = contacts.len();
        self.board.advance(&contacts, [target, target], self.settings.step);
        self.ticks += 1;
        let finite = self.board.bodies().iter().all(|b| {
            let p = b.rates.position;
            p.x.is_finite() && p.y.is_finite() && p.z.is_finite()
        });
        if finite { Ok(()) } else { Err(StepError::NonFinite { tick: self.ticks }) }
    }

    fn refresh_patch(&mut self, probe: &mut dyn GroundProbe) {
        let deck = self.deck_position_gta();
        let stale = self
            .patch
            .as_ref()
            .is_none_or(|p| p.needs_resample(deck, &self.patch_settings));
        if stale {
            let patch = terrain::sample(
                probe,
                &self.frame,
                deck,
                &self.patch_settings,
                self.settings.floor_material,
            );
            self.world = terrain::world(&patch);
            self.patch = Some(patch);
        } else if let Some(p) = self.patch.as_mut() {
            p.age_ticks += 1;
        }
    }

    fn apply_controls(&mut self, controls: Controls) {
        let deck = self.deck();
        let forward = column(deck.basis.columns[2]);
        let velocity = self.board.bodies()[BodyId::Deck.index()].rates.linear_velocity;
        let speed = dot(velocity, forward);
        if controls.push && speed < self.stock.maximum_pushable_speed {
            self.push_force(scale(forward, self.tuning.push_force));
        }
        if controls.brake && speed.abs() > self.stock.slow_speed {
            self.push_force(scale(forward, -speed.signum() * self.stock.foot_brake_force));
        }
    }

    fn push_force(&mut self, force_world: Vector3) {
        self.board.forces_mut().append(QueuedPointForce {
            tag: 0,
            force_world,
            point_body: Vector3::ZERO,
        });
    }

    pub fn deck(&self) -> RetailAffineTransform {
        self.board.part_transforms()[BodyId::Deck.index()]
    }

    pub fn deck_position_gta(&self) -> GtaVec {
        self.frame.to_gta(self.deck().translation)
    }

    pub fn speed(&self) -> f32 {
        let v = self.board.bodies()[BodyId::Deck.index()].rates.linear_velocity;
        dot(v, v).sqrt()
    }

    pub fn patch(&self) -> Option<&Patch> {
        self.patch.as_ref()
    }
}

/// Board contact query settings of the original host (ground.rs).
fn query_settings() -> WorldContactSettings {
    WorldContactSettings {
        volume_padding: 0.05,
        maximum_separating_distance: 0.5,
        edge_cos_bend_normal_threshold: 0.999,
        convexity_epsilon: 0.01,
        is_object: false,
    }
}

fn retention_settings() -> ContactRetentionSettings {
    ContactRetentionSettings {
        capacity: u32::MAX,
        duplicate_distance_squared: -1.0,
        deferred_reduction: true,
    }
}

fn column(c: [f32; 3]) -> Vector3 {
    Vector3::new(c[0], c[1], c[2])
}
fn dot(a: Vector3, b: Vector3) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z
}
fn scale(a: Vector3, s: f32) -> Vector3 {
    Vector3::new(a.x * s, a.y * s, a.z * s)
}
