//! SkateGTA.ini, a flat `key = value` file next to the .asi.
use crate::{sim::TestControlTuning, terrain::PatchSettings};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub skate3rust_dir: Option<PathBuf>,
    pub asset_root: Option<PathBuf>,
    pub toggle_key: u32,
    pub board_model: String,
    pub model_yaw_offset: f32,
    pub model_z_offset: f32,
    pub ped_z_offset: f32,
    pub probe_flags: i32,
    pub debug_draw: bool,
    pub tuning: TestControlTuning,
    pub patch: PatchSettings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            skate3rust_dir: None,
            asset_root: None,
            toggle_key: 0x74, // F5
            board_model: "p_defilied_ragdoll_01_s".into(),
            model_yaw_offset: 0.0,
            model_z_offset: 0.0,
            ped_z_offset: 1.0,
            probe_flags: 1 | 16,
            debug_draw: true,
            tuning: TestControlTuning::default(),
            patch: PatchSettings::default(),
        }
    }
}

pub const TEMPLATE: &str = "\
; SkateGTA - Skate 3 Rust Engine physics in GTA V (ScriptHookV)
; Folder that contains skate3rust.exe. Run it once and pick your default.xex first.
Skate3RustDir = C:\\Games\\skate3rust
; Or point directly at the converted assets folder (overrides Skate3RustDir):
; AssetRoot = C:\\Games\\skate3rust\\data\\installations\\<id>\\assets

; Virtual-key code to get on/off the board (0x74 = F5)
ToggleKey = 0x74
; Object used as the board model. Vanilla GTA has no skateboard; replace this
; object with a skateboard .ydr (as SkateV does) or pick another prop.
BoardModel = p_defilied_ragdoll_01_s
ModelYawOffset = 0
ModelZOffset = 0
PedZOffset = 1.0
; Draw the solver's deck, wheels and the sampled ground patch
DebugDraw = 1

; Phase-1 test controls (replaced by Skate 3 riding in phase 2)
PushForce = 40
SteerAngle = 0.3
InvertSteer = 0

; Ground sampling around the board
ProbeFlags = 17
PatchCells = 8
PatchSpacing = 0.4
";

impl Config {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut c = Self::default();
        for (number, line) in text.lines().enumerate() {
            let line = line.trim().trim_start_matches('\u{feff}');
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') || line.starts_with('[') {
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| format!("SkateGTA.ini line {}: expected key = value", number + 1))?;
            let (key, value) = (key.trim(), value.trim());
            let err = |what: &str| format!("SkateGTA.ini line {}: {key} {what}", number + 1);
            let float = || value.parse::<f32>().ok().filter(|v| v.is_finite()).ok_or_else(|| err("must be a number"));
            let flag = || match value {
                "1" | "true" | "yes" => Ok(true),
                "0" | "false" | "no" => Ok(false),
                _ => Err(err("must be 0 or 1")),
            };
            match key.to_ascii_lowercase().as_str() {
                "skate3rustdir" => c.skate3rust_dir = Some(PathBuf::from(value)),
                "assetroot" => c.asset_root = Some(PathBuf::from(value)),
                "togglekey" => c.toggle_key = parse_int(value).ok_or_else(|| err("must be an integer"))? as u32,
                "boardmodel" => c.board_model = value.to_string(),
                "modelyawoffset" => c.model_yaw_offset = float()?,
                "modelzoffset" => c.model_z_offset = float()?,
                "pedzoffset" => c.ped_z_offset = float()?,
                "debugdraw" => c.debug_draw = flag()?,
                "pushforce" => c.tuning.push_force = float()?,
                "steerangle" => c.tuning.steer_angle = float()?,
                "invertsteer" => c.tuning.invert_steer = flag()?,
                "probeflags" => c.probe_flags = parse_int(value).ok_or_else(|| err("must be an integer"))? as i32,
                "patchcells" => {
                    let n = parse_int(value).filter(|n| (2..=32).contains(n)).ok_or_else(|| err("must be 2..32"))?;
                    c.patch.cells = n as usize;
                }
                "patchspacing" => c.patch.spacing = float()?.clamp(0.05, 2.0),
                _ => return Err(err("is not a known setting")),
            }
        }
        Ok(c)
    }
}

fn parse_int(value: &str) -> Option<i64> {
    match value.strip_prefix("0x").or_else(|| value.strip_prefix("0X")) {
        Some(hex) => i64::from_str_radix(hex, 16).ok(),
        None => value.parse().ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_parses() {
        let c = Config::parse(TEMPLATE).unwrap();
        assert_eq!(c.toggle_key, 0x74);
        assert_eq!(c.probe_flags, 17);
        assert!(c.skate3rust_dir.is_some() && c.asset_root.is_none());
    }

    #[test]
    fn unknown_keys_are_reported() {
        assert!(Config::parse("Bogus = 1").unwrap_err().contains("Bogus"));
        assert!(Config::parse("PushForce = abc").is_err());
    }
}
