//! The complete Skate 3 gameplay (skate-gameplay) placed in the GTA world:
//! a skate-space frame at the player's feet, ground patches that follow the
//! board and the skater, and GTA-space views of the results.
use crate::{
    coords::{EntityAxes, Frame, GtaVec},
    far::{FarField, FarSettings},
    terrain::{GroundProbe, PatchSettings, Patches},
};
use skate_core::{math::Basis3, physics::contact::RetailContactMaterial};
use skate_gameplay::host::{Game, Mode, SPAWN_GROUND_HEIGHT};
use std::{path::Path, time::Duration};

/// The original host's static floor material: zero friction and full
/// restitution, so the moving volume's own material decides each contact.
const FLOOR: RetailContactMaterial = RetailContactMaterial {
    static_friction: 0.0,
    dynamic_friction: 0.0,
    restitution: 1.0,
};

pub struct Ride {
    /// Boxed: the gameplay state is far larger than a script fiber's stack.
    pub game: Box<Game>,
    pub frame: Frame,
    pub patches: Patches,
    pub far: FarField,
}

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
        let frame = Frame::facing(ground.add(GtaVec::new(0.0, 0.0, -SPAWN_GROUND_HEIGHT)), heading_degrees);
        let mut ride = Self { game, frame, patches: Patches::new(patch, FLOOR), far: FarField::new(FarSettings::default()) };
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
        if self.patches.refresh(probe, &self.frame, &centers) {
            let world = crate::terrain::world_of(&self.patches.patches);
            let game = &mut self.game;
            crate::bigstack::run(move || game.set_world(world))?;
        }
        self.far.update(probe, self.hips_position());
        let queries = self.far.queries(self.frame, self.patches.rects());
        self.game.set_external_queries(Some(std::sync::Arc::new(queries)));
        Ok(())
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
    pub fn advance_game(&mut self, elapsed: Duration, max_ticks: u32, pad: [f32; 18]) -> Result<u32, String> {
        let game = &mut self.game;
        crate::bigstack::run(|| game.advance(elapsed, max_ticks, pad))
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
