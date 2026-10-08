//! The decoded audio described by private/audio/audio_manifest.json (upstream
//! `game_audio/library.rs`, trimmed to what the player's native audio reads: no
//! overlays, ambience, world emitters or speech).
use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Debug, Deserialize)]
pub(crate) struct Entry {
    pub file: String,
}

/// The native AEMS runtime's inputs (audio_export.aems_files): Csis projects in install order and
/// module banks by stem, as files under the audio folder.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct AemsFiles {
    #[serde(default)]
    pub projects: Vec<String>,
    #[serde(default)]
    pub banks: BTreeMap<String, String>,
    /// `MixMapSK8.mxb` (setup copies it next to the banks; absent before 2026-10-02 installs).
    #[serde(default)]
    pub mixmap: Option<String>,
    /// SPLC patch trees by bank stem (`audio_export.splice_trees`; absent before 2026-10-02).
    #[serde(default)]
    pub splice: BTreeMap<String, String>,
}

/// A rolling grain member: the whole recording and its raw `.grain` member for the native grain
/// player (the manifest's speed `bands`, the removed interim loop's, are ignored).
#[derive(Debug, Deserialize)]
struct Grain {
    #[serde(default)]
    file: Option<String>,
    #[serde(default)]
    grain: Option<String>,
}

/// The vault's grain tuning (audio_export.grain_tuning): every float exactly as stored.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct GrainTuningJson {
    #[serde(default)]
    surfaces: BTreeMap<String, SurfaceJson>,
    #[serde(default)]
    default: SurfaceJson,
    #[serde(default)]
    owner: OwnerJson,
    /// Collision material (tag − 1) → rolling surface 1–14 (`Sk8::AudioSurfaceMap`).
    /// Read only by the vault check (`grain_for_matches_the_vault_surface_map_and_the_bed_loads`).
    #[serde(default)]
    #[cfg_attr(not(test), allow(dead_code))]
    pub surface_map: Vec<u32>,
}

#[derive(Debug, Default, Clone, Deserialize)]
struct SurfaceJson {
    max_kmh: Option<f32>,
    bezier: Option<[f32; 4]>,
    params: Option<Vec<Vec<f32>>>,
    turn_cap: Option<f32>,
    turn_rise_step: Option<f32>,
    turn_fall_step: Option<f32>,
    special_gain: Option<f32>,
    special_shift_hz: Option<f32>,
    b_slope_gain: Option<f32>,
    b_slope_ramp_kmh: Option<f32>,
    a_shift_per_slope_hz: Option<f32>,
    b_base_shift_hz: Option<f32>,
    b_shift_per_slope_hz: Option<f32>,
    push_ramp_kmh: Option<f32>,
    push_scale_low: Option<f32>,
    push_scale_high: Option<f32>,
    push_shift_low_hz: Option<f32>,
    push_shift_high_hz: Option<f32>,
    push_scale_attack_ms: Option<f32>,
    push_scale_hold_ms: Option<f32>,
    push_scale_return_ms: Option<f32>,
    push_shift_attack_ms: Option<f32>,
    push_shift_hold_ms: Option<f32>,
    push_shift_return_ms: Option<f32>,
    slope_down_divisor: Option<f32>,
    slope_up_divisor: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
struct OwnerJson {
    rocket_start_kmh: Option<f32>,
    rocket_top_kmh: Option<f32>,
    rocket_gain_word: Option<i32>,
    rocket_params: Option<Vec<f32>>,
    g1_level_start_kmh: Option<f32>,
    g1_level_end_kmh: Option<f32>,
    g1_level_floor: Option<f32>,
    // The board chains' tuning (`grain::chain::ChainTuning`; absent before the chain export).
    g3_send_start_kmh: Option<f32>,
    g3_send_end_kmh: Option<f32>,
    g3_send_max: Option<f32>,
    g3_clip: Option<f32>,
    g3_shelf_hz: Option<f32>,
    g3_shelf_gain: Option<f32>,
    wobble_start_kmh: Option<f32>,
    wobble_end_kmh: Option<f32>,
    wobble_a_ms_low: Option<i32>,
    wobble_a_ms_high: Option<i32>,
    wobble_a_low: Option<f32>,
    wobble_a_high: Option<f32>,
    wobble_b_ms_low: Option<i32>,
    wobble_b_ms_high: Option<i32>,
    wobble_b_low: Option<f32>,
    wobble_b_high: Option<f32>,
}

impl SurfaceJson {
    /// The member's tuning with every missing field from `default`; None if a field is missing
    /// from both (an install without the vault tuning).
    fn tuning(&self, d: &SurfaceJson) -> Option<skate_audio::grain::board::SurfaceTuning> {
        use skate_audio::grain::{GrainParams, board::{PushTuning, SurfaceTuning}};
        let f = |a: Option<f32>, b: Option<f32>| a.or(b);
        let params = self.params.as_ref().or(d.params.as_ref())?;
        let params = [GrainParams::from_slice(params.first()?)?, GrainParams::from_slice(params.get(1)?)?];
        Some(SurfaceTuning {
            max_kmh: f(self.max_kmh, d.max_kmh)?,
            bezier: self.bezier.or(d.bezier)?,
            params,
            turn_cap: f(self.turn_cap, d.turn_cap)?,
            turn_rise_step: f(self.turn_rise_step, d.turn_rise_step)?,
            turn_fall_step: f(self.turn_fall_step, d.turn_fall_step)?,
            special_gain: f(self.special_gain, d.special_gain)?,
            special_shift_hz: f(self.special_shift_hz, d.special_shift_hz)?,
            b_slope_gain: f(self.b_slope_gain, d.b_slope_gain)?,
            b_slope_ramp_kmh: f(self.b_slope_ramp_kmh, d.b_slope_ramp_kmh)?,
            a_shift_per_slope_hz: f(self.a_shift_per_slope_hz, d.a_shift_per_slope_hz)?,
            b_base_shift_hz: f(self.b_base_shift_hz, d.b_base_shift_hz)?,
            b_shift_per_slope_hz: f(self.b_shift_per_slope_hz, d.b_shift_per_slope_hz)?,
            // Installs staged before these fields were read fall back to the vault's values.
            slope_divisors: (
                f(self.slope_down_divisor, d.slope_down_divisor).unwrap_or(-10.0),
                f(self.slope_up_divisor, d.slope_up_divisor).unwrap_or(10.0),
            ),
            push: PushTuning {
                ramp_kmh: f(self.push_ramp_kmh, d.push_ramp_kmh)?,
                scale_low: f(self.push_scale_low, d.push_scale_low)?,
                scale_high: f(self.push_scale_high, d.push_scale_high)?,
                shift_low_hz: f(self.push_shift_low_hz, d.push_shift_low_hz)?,
                shift_high_hz: f(self.push_shift_high_hz, d.push_shift_high_hz)?,
                scale_ms: [
                    f(self.push_scale_attack_ms, d.push_scale_attack_ms)?,
                    f(self.push_scale_hold_ms, d.push_scale_hold_ms)?,
                    f(self.push_scale_return_ms, d.push_scale_return_ms)?,
                ],
                shift_ms: [
                    f(self.push_shift_attack_ms, d.push_shift_attack_ms)?,
                    f(self.push_shift_hold_ms, d.push_shift_hold_ms)?,
                    f(self.push_shift_return_ms, d.push_shift_return_ms)?,
                ],
            },
        })
    }
}

#[derive(Debug, Deserialize)]
struct Manifest {
    version: u32,
    grains: BTreeMap<String, Grain>,
    wheels: BTreeMap<String, Entry>,
    banks: BTreeMap<String, Vec<Entry>>,
    #[serde(default)]
    aems: AemsFiles,
    #[serde(default)]
    grain_player: GrainTuningJson,
    #[serde(default)]
    player_tuning: PlayerTuningJson,
    #[serde(default)]
    bus_tuning: BusTuningJson,
}


#[derive(Debug, Default, Deserialize)]
struct JitterJson {
    /// The collection key (16 hex digits; absent before the eEQChain bus export).
    #[serde(default)]
    key: String,
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    id: i64,
    params: Vec<f32>,
}

#[derive(Debug, Default, Deserialize)]
struct SeamJson {
    gain_low: Option<f32>,
    gain_high: Option<f32>,
    ms_low: Option<i32>,
    ms_high: Option<i32>,
    // Class_Seams' pattern fields (absent before the seams export).
    gain: Option<f32>,
    angle: Option<i32>,
    grid_z: Option<f32>,
    grid_x: Option<f32>,
    class: Option<i32>,
    mode: Option<i32>,
    min_frames: Option<i32>,
    speed_threshold: Option<f32>,
    spacing: Option<f32>,
    level: Option<f32>,
    surface3_scale: Option<f32>,
}

/// The native buses' vault tuning (`audio_export.bus_tuning`; optional).
#[derive(Debug, Default, Deserialize)]
struct BusTuningJson {
    /// Reverb preset key (16 hex digits) → its 44 record values by offset.
    #[serde(default)]
    reverb: BTreeMap<String, Vec<f32>>,
    #[serde(default)]
    eq_buses: Vec<EqBusJson>,
    /// The two FlangeSub effect returns' records (A, B), nine values each by record offset.
    #[serde(default)]
    flange: Vec<Vec<f32>>,
}

#[derive(Debug, Default, Deserialize)]
struct EqBusJson {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    clip: f32,
    #[serde(default)]
    ranges: Vec<[f32; 2]>,
}

#[derive(Debug, Default, Deserialize)]
struct GrindJson {
    v: Vec<f32>,
    f: Vec<f32>,
    /// The grind contact sounds (exported since 2026-10-03; optional).
    #[serde(default)]
    metal: bool,
    on: Option<GrindContactJson>,
    off: Option<GrindContactJson>,
}

#[derive(Debug, Default, Deserialize)]
struct GrindContactJson {
    ids: Vec<i32>,
    gain: Vec<f32>,
    level: Vec<f32>,
    pitch: Vec<f32>,
}

impl GrindContactJson {
    fn contact(&self) -> skate_audio::player::tuning::GrindContact {
        let d = skate_audio::player::tuning::GrindContact::default();
        let two = |v: &[f32], d: [f32; 2]| if v.len() == 2 { [v[0], v[1]] } else { d };
        skate_audio::player::tuning::GrindContact {
            ids: std::array::from_fn(|i| self.ids.get(i).copied().unwrap_or(-1)),
            gain: std::array::from_fn(|i| self.gain.get(i).copied().unwrap_or(1.0)),
            level: two(&self.level, d.level),
            pitch: two(&self.pitch, d.pitch),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct PlayerTuningJson {
    #[serde(default)]
    surface_table: Vec<Vec<i32>>,
    #[serde(default)]
    jitter: Vec<JitterJson>,
    #[serde(default)]
    seam_wobbles: Vec<SeamJson>,
    #[serde(default)]
    grind: Vec<GrindJson>,
    #[serde(default)]
    landing_materials: Vec<u32>,
    wheel_bucket_high: Option<f32>,
    wheel_bucket_low: Option<f32>,
    jump_thresholds: Option<[f32; 2]>,
    /// Name hash (16 hex digits) → audio trick id.
    #[serde(default)]
    audio_tricks: BTreeMap<String, i32>,
    /// The scorables' second audio trick field (`A2C5C22C5BE725F8`, state `+352`), keyed the same way.
    #[serde(default)]
    audio_tricks_2: BTreeMap<String, i32>,
    /// The collision manager's material table and the posters' vault values
    /// (`audio_export.collision_tuning`; optional).
    #[serde(default)]
    collision: CollisionJson,
    /// The grind contact sounds' eEQChain bus (`D1A87641CCB98787`; optional).
    grind_contact_eq: Option<u8>,
}

#[derive(Debug, Default, Deserialize)]
struct CollisionJson {
    #[serde(default)]
    materials: Vec<MaterialJson>,
    #[serde(default)]
    posters: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Default, Deserialize)]
struct MaterialJson {
    #[serde(default = "no_kind")]
    kind: i32,
    #[serde(default)]
    ids: Vec<i32>,
    #[serde(default)]
    gain: i32,
    pitch: Option<i32>,
    #[serde(default)]
    pitch_flag: bool,
    #[serde(default)]
    pitch_alt: i32,
    #[serde(default)]
    category: i32,
    #[serde(default)]
    landing: bool,
    #[serde(default)]
    windows: Vec<i32>,
    #[serde(default)]
    scale: f32,
    #[serde(default)]
    bands: Vec<f32>,
    /// The footstep layer (`player::footsteps::FootstepMaterial`; absent before that export).
    #[serde(default)]
    footsteps: bool,
    step_gain: Option<i32>,
    step_landing_gain: Option<i32>,
}

fn no_kind() -> i32 {
    -1
}

impl CollisionJson {
    fn tuning(&self, surface_table: &[Vec<i32>]) -> skate_audio::player::collision::CollisionTuning {
        use skate_audio::player::collision::{CollisionTuning, Material};
        CollisionTuning {
            materials: self
                .materials
                .iter()
                .map(|m| Material {
                    kind: m.kind,
                    ids: std::array::from_fn(|i| m.ids.get(i).copied().unwrap_or(0)),
                    gain: m.gain,
                    pitch: m.pitch.unwrap_or(4096),
                    pitch_flag: m.pitch_flag,
                    pitch_alt: m.pitch_alt,
                    category: m.category,
                    landing: m.landing,
                    windows: std::array::from_fn(|i| m.windows.get(i).copied().unwrap_or(0)),
                    scale: m.scale,
                    bands: std::array::from_fn(|i| m.bands.get(i).copied().unwrap_or(0.0)),
                })
                .collect(),
            // AudioSurfaceMap word 7 (+28): the collision class.
            surface_class: surface_table.iter().map(|r| r.get(7).copied().unwrap_or(0)).collect(),
            // Words 12..16 (+48..+64): the collision voices' eEQChain bus by tier / class.
            surface_eq: surface_table.iter().map(|r| std::array::from_fn(|i| r.get(12 + i).copied().unwrap_or(8))).collect(),
        }
    }

    /// The posters' vault values over the retail defaults.
    pub(crate) fn contacts(&self) -> skate_audio::player::contacts::ContactsTuning {
        let mut c = skate_audio::player::contacts::ContactsTuning::default();
        let f = |k: &str| self.posters.get(k).and_then(serde_json::Value::as_f64).map(|v| v as f32);
        let ids = |k: &str| -> Option<Vec<u32>> {
            self.posters.get(k)?.as_array().map(|a| a.iter().filter_map(|v| v.as_u64().map(|v| v as u32)).collect())
        };
        let three = |v: Option<Vec<u32>>, d: [u32; 3]| v.filter(|v| v.len() == 3).map_or(d, |v| [v[0], v[1], v[2]]);
        let two = |v: Option<Vec<u32>>, d: [u32; 2]| v.filter(|v| v.len() == 2).map_or(d, |v| [v[0], v[1]]);
        c.grind_split = f("grind_split").unwrap_or(c.grind_split);
        c.grind_high = f("grind_high").unwrap_or(c.grind_high);
        c.landing_air = f("landing_air").unwrap_or(c.landing_air);
        c.landing_board = f("landing_board").map_or(c.landing_board, |v| v as i32);
        c.landing_split = f("landing_split").unwrap_or(c.landing_split);
        c.landing_high = f("landing_high").unwrap_or(c.landing_high);
        c.landing_scale = [f("landing_scale_a").unwrap_or(c.landing_scale[0]), f("landing_scale_b").unwrap_or(c.landing_scale[1])];
        c.deck_cooldown = f("deck_cooldown").unwrap_or(c.deck_cooldown);
        c.scuff_speed = f("scuff_speed").unwrap_or(c.scuff_speed);
        c.scuff_ids = two(ids("scuff_ids"), c.scuff_ids);
        c.scuff_ids_soft = two(ids("scuff_ids_soft"), c.scuff_ids_soft);
        c.tap_off_ms = f("tap_off_ms").unwrap_or(c.tap_off_ms);
        c.tap_speed = f("tap_speed").unwrap_or(c.tap_speed);
        c.tap_mid = f("tap_mid").unwrap_or(c.tap_mid);
        c.tap_high = f("tap_high").unwrap_or(c.tap_high);
        c.tap_ids = [
            three(ids("tap_first"), c.tap_ids[0]),
            three(ids("tap_second"), c.tap_ids[1]),
            three(ids("tap_both"), c.tap_ids[2]),
            three(ids("tap_special"), c.tap_ids[3]),
        ];
        c.tap_ids_soft = [
            three(ids("tap_first_soft"), c.tap_ids_soft[0]),
            three(ids("tap_second_soft"), c.tap_ids_soft[1]),
            three(ids("tap_other_soft"), c.tap_ids_soft[2]),
        ];
        // The push foot's plant / lift ids by material kind, their eEQChain bus; the body poster's
        // cooldown and pad thresholds (exported since 2026-10-03; the retail defaults otherwise).
        let five = |v: Option<Vec<u32>>, d: [u32; 5]| v.filter(|v| v.len() == 5).map_or(d, |v| [v[0], v[1], v[2], v[3], v[4]]);
        c.plant_ids = five(ids("plant_ids"), c.plant_ids);
        c.lift_ids = five(ids("lift_ids"), c.lift_ids);
        c.plant_eq = f("plant_eq").map_or(c.plant_eq, |v| v as u8);
        c.body_cooldown = f("body_cooldown").unwrap_or(c.body_cooldown);
        // The bridge's speed graph (exported since 2026-10-03; the stock vault's words otherwise).
        let eight = |k: &str| -> Option<[f32; 8]> {
            let a = self.posters.get(k)?.as_array()?;
            let v: Vec<f32> = a.iter().filter_map(|v| v.as_f64().map(|v| v as f32)).collect();
            (v.len() == 8).then(|| std::array::from_fn(|i| v[i]))
        };
        if let (Some(x), Some(y)) = (eight("body_speed_x"), eight("body_speed_y")) {
            c.body_speed_curve = skate_audio::player::contacts::SpeedGraph8 { x, y };
        }
        let pair = |a: &str, b: &str, d: [f32; 2]| [f(a).unwrap_or(d[0]), f(b).unwrap_or(d[1])];
        c.body_110 = [pair("body_110_head_low", "body_110_head_high", c.body_110[0]), pair("body_110_torso_low", "body_110_torso_high", c.body_110[1])];
        c.body_111 = pair("body_111_low", "body_111_high", c.body_111);
        c.body_112 = [pair("body_112_low0", "body_112_high0", c.body_112[0]), pair("body_112_low1", "body_112_high1", c.body_112[1])];
        c
    }
}

impl PlayerTuningJson {
    fn tuning(&self) -> skate_audio::player::tuning::PlayerTuning {
        use skate_audio::player::tuning::{GrindSurface, JitterParams, PlayerTuning, SeamWobble};
        let d = PlayerTuning::default();
        let four = |v: &[f32]| -> [f32; 4] { std::array::from_fn(|i| v.get(i).copied().unwrap_or(1.0)) };
        PlayerTuning {
            surface_table: self.surface_table.iter().map(|r| std::array::from_fn(|i| r.get(i).copied().unwrap_or(0))).collect(),
            jitter: self.jitter.iter().filter(|j| j.params.len() == 4).map(|j| JitterParams {
                enabled: j.enabled,
                id: j.id.clamp(0, 15) as usize,
                centre: j.params[0],
                range: j.params[1],
                max_step: j.params[2],
                min_step: j.params[3],
            }).collect(),
            // A pattern without a collection reads the image's zero block (all fields 0).
            seam_wobbles: self.seam_wobbles.iter().map(|w| SeamWobble {
                gain_low: w.gain_low.unwrap_or(0.0),
                gain_high: w.gain_high.unwrap_or(0.0),
                ms_low: w.ms_low.unwrap_or(0),
                ms_high: w.ms_high.unwrap_or(0),
            }).collect(),
            grind: self.grind.iter().map(|g| GrindSurface {
                v: four(&g.v),
                f: four(&g.f),
                metal: g.metal,
                on: g.on.as_ref().map_or_else(Default::default, GrindContactJson::contact),
                off: g.off.as_ref().map_or_else(Default::default, GrindContactJson::contact),
            }).collect(),
            grind_contact_eq: self.grind_contact_eq.unwrap_or(d.grind_contact_eq),
            landing_materials: self.landing_materials.clone(),
            wheel_bucket_high: self.wheel_bucket_high.unwrap_or(d.wheel_bucket_high),
            wheel_bucket_low: self.wheel_bucket_low.unwrap_or(d.wheel_bucket_low),
            jump_thresholds: self.jump_thresholds.unwrap_or(d.jump_thresholds),
            audio_tricks: self.audio_tricks.iter().filter_map(|(k, v)| Some((u64::from_str_radix(k, 16).ok()?, *v))).collect(),
            audio_tricks_2: self.audio_tricks_2.iter().filter_map(|(k, v)| Some((u64::from_str_radix(k, 16).ok()?, *v))).collect(),
            // The rolling layers', the Tricks component's and Class_Treatment's vault words: the
            // defaults are the user's vault values (not exported by setup yet).
            rolling: Default::default(),
            tricks: Default::default(),
            treatment: Default::default(),
            collision: self.collision.tuning(&self.surface_table),
            seam_patterns: self.seam_wobbles.iter().map(|w| skate_audio::player::tuning::SeamPattern {
                gain: w.gain.unwrap_or(0.0),
                angle: w.angle.unwrap_or(0),
                grid_z: w.grid_z.unwrap_or(0.0),
                grid_x: w.grid_x.unwrap_or(0.0),
                class: w.class.unwrap_or(0),
                mode: w.mode.unwrap_or(0),
                min_frames: w.min_frames.unwrap_or(0),
                speed_threshold: w.speed_threshold.unwrap_or(0.0),
                spacing: w.spacing.unwrap_or(0.0),
                level: w.level.unwrap_or(0.0),
            }).collect(),
            seam_surface3_scale: self.seam_wobbles.first().and_then(|w| w.surface3_scale).unwrap_or(1.0),
            eq_jitter: {
                let channels: Vec<&JitterJson> = self.jitter.iter().filter(|j| j.params.len() == 4).collect();
                skate_audio::player::tuning::EQ_JITTER_KEYS.map(|k| {
                    channels.iter().position(|j| u64::from_str_radix(&j.key, 16).ok() == Some(k))
                })
            },
        }
    }
}

/// A loaded sound. `key` identifies the file for per-sound voice limits;
/// A PCM16 WAV as planar f32 (−1..1), or None if it is not one.
pub(crate) fn wav_pcm(bytes: &[u8]) -> Option<skate_audio::mixer::Pcm> {
    let (mut channels, mut rate, mut bits) = (0usize, 0u32, 0u16);
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().ok()?) as usize;
        let body = at + 8;
        match &bytes[at..at + 4] {
            b"fmt " if body + 16 <= bytes.len() => {
                channels = usize::from(u16::from_le_bytes([bytes[body + 2], bytes[body + 3]]));
                rate = u32::from_le_bytes(bytes[body + 4..body + 8].try_into().ok()?);
                bits = u16::from_le_bytes([bytes[body + 14], bytes[body + 15]]);
            }
            b"data" if channels > 0 && bits == 16 => {
                let data = &bytes[body..(body + size).min(bytes.len())];
                let mut planar = vec![Vec::with_capacity(data.len() / (2 * channels)); channels];
                for (i, s) in data.chunks_exact(2).enumerate() {
                    planar[i % channels].push(f32::from(i16::from_le_bytes([s[0], s[1]])) / 32768.0);
                }
                return Some(skate_audio::mixer::Pcm { rate, channels: planar });
            }
            _ => {}
        }
        at = body + size + (size & 1);
    }
    None
}

/// A bank's decoded samples by S10A slot (None where a WAV is missing or unreadable).
pub(crate) type BankPcm = Vec<Option<Arc<skate_audio::mixer::Pcm>>>;

/// Read and decode the WAVs in order (`Library::bank_pcm` and `BankSource::load`).
fn decode_wavs<'a>(files: impl Iterator<Item = &'a Path>) -> BankPcm {
    files
        .map(|file| {
            std::fs::read(file).ok().and_then(|bytes| wav_pcm(&bytes)).map(Arc::new)
        })
        .collect()
}

/// Everything needed to read, parse and decode one AEMS bank.
#[derive(Clone, Debug)]
pub(crate) struct BankSource {
    stem: String,
    file: String,
    abk: PathBuf,
    wavs: Vec<PathBuf>,
}

impl BankSource {
    /// The `.abk` parsed and its WAVs decoded (a missing WAV plays silence of its length).
    pub(crate) fn load(&self) -> Result<(skate_audio::formats::Bank, BankPcm), String> {
        let bytes = std::fs::read(&self.abk).map_err(|e| format!("{}: {e}", self.file))?;
        let bank = skate_audio::formats::Bank::parse(&self.stem, bytes).map_err(|e| e.to_string())?;
        Ok((bank, decode_wavs(self.wavs.iter().map(PathBuf::as_path))))
    }
}

pub struct Library {
    root: PathBuf,
    manifest: Manifest,
}

impl Library {
    /// `asset_root/private/audio/audio_manifest.json`.
    pub fn load(asset_root: &Path) -> Result<Self, String> {
        let root = asset_root.join("private/audio");
        let path = root.join("audio_manifest.json");
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        if manifest.version < 5 {
            return Err(format!("{}: manifest version {} has no AEMS banks (rerun the Skate 3 Rust setup)", path.display(), manifest.version));
        }
        Ok(Self { root, manifest })
    }

    pub(crate) fn path(&self, file: &str) -> PathBuf {
        self.root.join(file)
    }

    pub(crate) fn read(&self, file: &str) -> std::io::Result<Vec<u8>> {
        std::fs::read(self.path(file))
    }

    /// Banks whose WAVs are all present (the rest load their programs and play silence).
    pub fn bank_complete(&self, stem: &str) -> bool {
        self.manifest.banks.get(stem).is_some_and(|e| !e.is_empty() && e.iter().all(|e| self.path(&e.file).is_file()))
    }

    pub(crate) fn has_bank(&self, stem: &str) -> bool {
        self.manifest.aems.banks.contains_key(stem)
    }

    /// The player components' vault tuning (empty tables on installs set up before it existed).
    pub(crate) fn player_tuning(&self) -> skate_audio::player::tuning::PlayerTuning {
        self.manifest.player_tuning.tuning()
    }

    /// The environment network's reverb presets by key and the eight eEQChain bus records (empty
    /// on installs set up before the bus export: no wet path, buses pass dry).
    pub(crate) fn bus_tuning(&self) -> (std::collections::HashMap<u64, skate_audio::bus::env::Preset>, Vec<skate_audio::bus::eqchain::EqRecord>) {
        let b = &self.manifest.bus_tuning;
        let presets = b.reverb.iter().filter_map(|(k, v)| {
            let key = u64::from_str_radix(k, 16).ok()?;
            (v.len() == 44).then(|| (key, skate_audio::bus::env::Preset(std::array::from_fn(|i| v[i]))))
        }).collect();
        let eq = b.eq_buses.iter().filter(|e| e.ranges.len() == 6).map(|e| skate_audio::bus::eqchain::EqRecord {
            enabled: e.enabled,
            clip: e.clip,
            ranges: std::array::from_fn(|i| e.ranges[i]),
        }).collect();
        (presets, eq)
    }

    /// The FlangeSub effect returns' presets (A, B), when the install has them.
    pub(crate) fn flange_presets(&self) -> Option<[skate_audio::bus::flange::FlangePreset; 2]> {
        let f = &self.manifest.bus_tuning.flange;
        let preset = |v: &Vec<f32>| (v.len() == 9).then(|| skate_audio::bus::flange::FlangePreset(std::array::from_fn(|i| v[i])));
        Some([preset(f.first()?)?, preset(f.get(1)?)?])
    }

    /// The footstep layer of materials 0..142 (empty on installs set up before that export: no
    /// material footstep layer, the step / walking layers still play).
    pub(crate) fn footstep_materials(&self) -> Vec<skate_audio::player::footsteps::FootstepMaterial> {
        let rows = &self.manifest.player_tuning.collision.materials;
        if rows.iter().all(|m| m.step_gain.is_none()) {
            return Vec::new();
        }
        rows.iter()
            .map(|m| {
                let ids: [i32; 7] = std::array::from_fn(|i| m.ids.get(i).copied().unwrap_or(0));
                skate_audio::player::footsteps::FootstepMaterial::from_collision(m.kind, &ids, m.footsteps, m.step_gain.unwrap_or(32767), m.step_landing_gain.unwrap_or(32767))
            })
            .collect()
    }

    /// The Contacts posters' vault values (collision posters, foot taps, scuffs).
    pub(crate) fn contacts_tuning(&self) -> skate_audio::player::contacts::ContactsTuning {
        self.manifest.player_tuning.collision.contacts()
    }

    /// A wheel-spin recording (`SFXObj_Wheels`) as planar PCM, decoded now.
    pub(crate) fn wheels_pcm(&self, name: &str) -> Option<Arc<skate_audio::mixer::Pcm>> {
        let entry = self.manifest.wheels.get(name)?;
        self.read(&entry.file).ok().and_then(|bytes| wav_pcm(&bytes)).map(Arc::new)
    }

    /// The native AEMS runtime's files (empty before manifest v5).
    pub(crate) fn aems(&self) -> &AemsFiles {
        &self.manifest.aems
    }

    /// A grain member's whole recording and raw `.grain` file (native grain player), if staged.
    pub(crate) fn grain_whole(&self, name: &str) -> Option<(&str, &str)> {
        let g = self.manifest.grains.get(name)?;
        Some((g.file.as_deref()?, g.grain.as_deref()?))
    }

    /// The vault tuning of a grain member (its collection over `default`).
    pub(crate) fn grain_tuning(&self, name: &str) -> Option<skate_audio::grain::board::SurfaceTuning> {
        let t = &self.manifest.grain_player;
        t.surfaces.get(name)?.tuning(&t.default)
    }

    /// The grain class's `default` collection as a tuning (what a bind with the `default` key uses:
    /// rolling surface 0, `player::rolling::member`).
    pub(crate) fn grain_default_tuning(&self) -> Option<skate_audio::grain::board::SurfaceTuning> {
        let t = &self.manifest.grain_player;
        t.default.tuning(&t.default)
    }

    /// The rocket layer's tuning (owner class `default`).
    pub(crate) fn rocket_tuning(&self) -> Option<skate_audio::grain::board::RocketTuning> {
        let o = &self.manifest.grain_player.owner;
        Some(skate_audio::grain::board::RocketTuning {
            start_kmh: o.rocket_start_kmh?,
            top_kmh: o.rocket_top_kmh?,
            gain_word: o.rocket_gain_word?,
            params: skate_audio::grain::GrainParams::from_slice(o.rocket_params.as_deref()?)?,
        })
    }

    /// Collision material → rolling surface (`Sk8::AudioSurfaceMap`, 95 entries; empty before
    /// 2026-10-02 installs).
    pub(crate) fn surface_map(&self) -> &[u32] {
        &self.manifest.grain_player.surface_map
    }

    /// The board chains' tuning (owner class `default`): the install's values over the vault
    /// defaults `ChainTuning::default()` holds (installs set up before the chain export).
    pub(crate) fn chain_tuning(&self) -> skate_audio::grain::chain::ChainTuning {
        let o = &self.manifest.grain_player.owner;
        let mut t = skate_audio::grain::chain::ChainTuning::default();
        let set = |v: &mut f32, x: Option<f32>| if let Some(x) = x { *v = x };
        let seti = |v: &mut i32, x: Option<i32>| if let Some(x) = x { *v = x };
        set(&mut t.send_start_kmh, o.g3_send_start_kmh);
        set(&mut t.send_end_kmh, o.g3_send_end_kmh);
        set(&mut t.send_max, o.g3_send_max);
        set(&mut t.level_start_kmh, o.g1_level_start_kmh);
        set(&mut t.level_end_kmh, o.g1_level_end_kmh);
        set(&mut t.level_floor, o.g1_level_floor);
        set(&mut t.wobble_start_kmh, o.wobble_start_kmh);
        set(&mut t.wobble_end_kmh, o.wobble_end_kmh);
        seti(&mut t.wobble[0].ms_low, o.wobble_a_ms_low);
        seti(&mut t.wobble[0].ms_high, o.wobble_a_ms_high);
        set(&mut t.wobble[0].low, o.wobble_a_low);
        set(&mut t.wobble[0].high, o.wobble_a_high);
        seti(&mut t.wobble[1].ms_low, o.wobble_b_ms_low);
        seti(&mut t.wobble[1].ms_high, o.wobble_b_ms_high);
        set(&mut t.wobble[1].low, o.wobble_b_low);
        set(&mut t.wobble[1].high, o.wobble_b_high);
        set(&mut t.clip, o.g3_clip);
        set(&mut t.shelf_hz, o.g3_shelf_hz);
        set(&mut t.shelf_gain, o.g3_shelf_gain);
        t
    }

    pub(crate) fn splice_bank(&self, stem: &str) -> Option<(skate_audio::splice::SpliceBank, Vec<Option<Arc<skate_audio::mixer::Pcm>>>)> {
        let file = self.manifest.aems.splice.get(stem)?;
        let bank = skate_audio::splice::SpliceBank::parse(&self.read(file).ok()?).ok()?;
        let pcm = self.bank_pcm(stem);
        (pcm.len() == bank.samples).then_some((bank, pcm))
    }

    /// A bank's decoded samples as planar f32 PCM by S10A slot (the WAV order); None where a file
    /// is missing or unreadable (the native runtime then plays silence of the right length).
    pub(crate) fn bank_pcm(&self, bank: &str) -> BankPcm {
        let Some(entries) = self.manifest.banks.get(bank) else { return Vec::new() };
        let paths: Vec<PathBuf> = entries.iter().map(|e| self.path(&e.file)).collect();
        decode_wavs(paths.iter().map(PathBuf::as_path))
    }

    pub(crate) fn bank_source(&self, stem: &str) -> Result<BankSource, String> {
        let file = self.manifest.aems.banks.get(stem).ok_or_else(|| format!("bank {stem} is not in the install"))?;
        let entries = self.manifest.banks.get(stem).map_or(&[][..], Vec::as_slice);
        Ok(BankSource { stem: stem.to_owned(), file: file.clone(), abk: self.path(file), wavs: entries.iter().map(|e| self.path(&e.file)).collect() })
    }
}
