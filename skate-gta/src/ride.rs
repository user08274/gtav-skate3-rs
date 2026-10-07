//! The complete Skate 3 gameplay (skate-gameplay) placed in the GTA world:
//! a skate-space frame at the player's feet, ground patches that follow the
//! board and the skater, and GTA-space views of the results.
use crate::{
    coords::{EntityAxes, Frame, GtaVec},
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
    pub game: Game,
    pub frame: Frame,
    pub patches: Patches,
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
        let game = Game::load(root, mode)?;
        let frame = Frame::facing(ground.add(GtaVec::new(0.0, 0.0, -SPAWN_GROUND_HEIGHT)), heading_degrees);
        let mut ride = Self { game, frame, patches: Patches::new(patch, FLOOR) };
        ride.refresh_world(probe);
        Ok(ride)
    }

    fn refresh_world(&mut self, probe: &mut dyn GroundProbe) {
        let centers = [self.deck_position(), self.hips_position()];
        if self.patches.refresh(probe, &self.frame, &centers) {
            self.game.set_world(crate::terrain::world_of(&self.patches.patches));
        }
    }

    pub fn advance(
        &mut self,
        elapsed: Duration,
        max_ticks: u32,
        pad: [f32; 18],
        probe: &mut dyn GroundProbe,
    ) -> Result<u32, String> {
        self.refresh_world(probe);
        self.game.advance(elapsed, max_ticks, pad)
    }

    pub fn deck_position(&self) -> GtaVec {
        self.frame.to_gta(self.game.deck().translation)
    }

    pub fn deck_axes(&self) -> EntityAxes {
        self.frame.entity_axes(self.game.deck().basis)
    }

    pub fn hips_position(&self) -> GtaVec {
        self.game
            .skater_body_positions()
            .first()
            .map(|p| self.frame.to_gta(*p))
            .unwrap_or_else(|| self.deck_position())
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
