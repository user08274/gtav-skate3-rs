//! The skater's Skate 3 sounds on the native AEMS runtime (`skate-audio`): upstream
//! `crates/skate-game/src/game_audio` (skate_events, player_audio, grain_bed, the
//! `native` host pass) without Bevy, the world emitters, ambience, speech or mods.
//!
//! The game publishes the retail audio-state record after every physics tick
//! ([`skate_events::observe`], called by [`crate::host::Game::tick`]); once per
//! rendered frame the host calls [`GameAudio::frame`], which runs upstream's
//! `mixmap_frame` → `mixmap_tick` → `grain_bed::update` pass on the steps taken.
//! The output device pulls 48 kHz stereo from [`GameAudio::runtime`]
//! (`Runtime::fill_stereo`) on its own thread.
pub(crate) mod grain_bed;
pub mod library;
pub(crate) mod player_audio;
pub(crate) mod skate_events;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use skate_audio::formats::Project;
use skate_audio::mixmap::{MixMap, keys};
use skate_audio::runtime::Runtime;

pub use library::Library;
use player_audio::PlayerAudio;

/// One 60 Hz physics step; the MixMap evaluates on every second one (the console's 30 Hz).
pub(crate) const MIX_STEP: f32 = 1.0 / 60.0;
/// The most host ticks one frame takes (upstream `native::MAX_STEPS_PER_FRAME`).
pub(crate) const MAX_STEPS_PER_FRAME: u32 = 4;

/// The runtime, shared with the audio output thread.
pub type Shared = Arc<Mutex<Runtime>>;

/// What started, for the host's log.
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub projects: usize,
    pub banks: Vec<String>,
    /// Banks whose WAVs are missing (their sounds play silence).
    pub silent_banks: Vec<String>,
    pub splice_banks: Vec<String>,
    pub rolling_bed: bool,
    pub wheels: bool,
    pub components: bool,
    pub contacts: bool,
}

/// The camera as the position controllers see it, in skate space (metres, Y up).
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub position: [f32; 3],
    pub forward: [f32; 3],
}

pub struct GameAudio {
    shared: Shared,
    library: Library,
    mixmap: MixMap,
    player: PlayerAudio,
    bed: Option<grain_bed::Bed>,
    cadence: skate_audio::mixmap::cadence::Cadence,
    holds: bool,
    banks: HashMap<String, usize>,
    pub report: Report,
}

impl GameAudio {
    /// The runtime with every project, the player's banks, the Splice trees, the wheel
    /// streams, the buses and the MixMap, in upstream's boot order (`native::start_from`).
    pub fn start(asset_root: &std::path::Path) -> Result<Self, String> {
        let library = Library::load(asset_root)?;
        let files = library.aems();
        if files.projects.is_empty() {
            return Err("the audio install has no AEMS projects".into());
        }
        let mut report = Report::default();
        let mut runtime = Runtime::new();
        for file in &files.projects {
            let bytes = library.read(file).map_err(|e| format!("{file}: {e}"))?;
            let project = Project::parse(file, &bytes).map_err(|e| format!("{file}: {e}"))?;
            runtime.install_project(&project);
            report.projects += 1;
        }
        let mxb = files.mixmap.as_ref().ok_or("the audio install has no MixMapSK8.mxb")?;
        let mixmap = MixMap::from_bytes(&library.read(mxb).map_err(|e| format!("{mxb}: {e}"))?).map_err(|e| format!("{mxb}: {e}"))?;
        // The buses: the reverb network (GTA has no Skate 3 regions: the default preset),
        // the eEQChain buses, the FlangeSub returns and the FootStep SubMix graphs.
        let (presets, eq) = library.bus_tuning();
        runtime.mixer.buses.env.presets = presets;
        runtime.mixer.buses.eq.set_records(&eq);
        runtime.mixer.buses.env.request(skate_audio::bus::env::DEFAULT_PRESET);
        if let Some([a, b]) = library.flange_presets() {
            runtime.mixer.buses.flange.set_presets(a, b);
        }
        runtime.mixer.buses.submix.enabled = true;
        let mut audio = Self {
            shared: Arc::new(Mutex::new(runtime)),
            bed: grain_bed::Bed::new(&library),
            player: PlayerAudio::new(library.player_tuning(), true),
            library,
            mixmap,
            cadence: Default::default(),
            holds: false,
            banks: HashMap::new(),
            report,
        };
        audio.report.rolling_bed = audio.bed.is_some();
        audio.ensure_bank("emitter_utility")?;
        audio.load_player_banks();
        audio.boot_utilities();
        if let Ok(mut rt) = audio.shared.lock() {
            rt.grains.chain_extras = true;
        }
        Ok(audio)
    }

    /// The runtime the output thread renders from.
    pub fn runtime(&self) -> Shared {
        self.shared.clone()
    }

    fn ensure_bank(&mut self, stem: &str) -> Result<usize, String> {
        if let Some(&id) = self.banks.get(stem) {
            return Ok(id);
        }
        let (bank, pcm) = self.library.bank_source(stem)?.load()?;
        let id = self.shared.lock().map_err(|_| "audio lock poisoned")?.load_bank(bank, pcm);
        self.banks.insert(stem.to_owned(), id);
        self.report.banks.push(stem.to_owned());
        if stem != "emitter_utility" && stem != skate_audio::player::seams::UTILITY_BANK && !self.library.bank_complete(stem) {
            self.report.silent_banks.push(stem.to_owned());
        }
        Ok(id)
    }

    /// Upstream `load_player_banks` + `load_optional_player_banks`.
    fn load_player_banks(&mut self) {
        for stem in player_audio::BANKS {
            match self.ensure_bank(stem) {
                Ok(id) => {
                    if let Ok(mut rt) = self.shared.lock() {
                        rt.mixer.set_bank_group(id, skate_audio::mixer::GROUP_PLAYER);
                    }
                }
                Err(_) => {
                    self.player.components = false;
                    return;
                }
            }
        }
        self.report.components = true;
        let mut optional = [false; 5];
        for (k, banks) in player_audio::OPTIONAL_BANKS.iter().enumerate() {
            for (i, stem) in banks.iter().enumerate() {
                if let Ok(id) = self.ensure_bank(stem) {
                    if let Ok(mut rt) = self.shared.lock() {
                        rt.mixer.set_bank_group(id, skate_audio::mixer::GROUP_PLAYER);
                    }
                    optional[k] |= i == 0;
                }
            }
        }
        let p = &mut self.player;
        (p.rolling_on, p.rattle_on, p.slide_on, p.tricks_on, p.treatment_on) = (optional[0], optional[1], optional[2], optional[3], optional[4]);
        let mut first = false;
        for (i, stem) in player_audio::SPLICE_BANKS.iter().enumerate() {
            if let Some((bank, pcm)) = self.library.splice_bank(stem) {
                if let Ok(mut rt) = self.shared.lock() {
                    let rt = &mut *rt;
                    rt.splice.load_bank(stem, bank, pcm, &mut rt.mixer);
                    first |= i == 0;
                    self.report.splice_banks.push((*stem).to_owned());
                }
            }
        }
        let streams: Vec<_> = player_audio::WHEEL_STREAMS.iter().map(|n| self.library.wheels_pcm(n)).collect();
        let wheels = streams.iter().all(Option::is_some);
        if let Ok(mut rt) = self.shared.lock() {
            rt.load_streams(streams);
        }
        let p = &mut self.player;
        p.contacts_on = first;
        p.set_footstep_materials(self.library.footstep_materials());
        p.footsteps_on = first;
        p.contact_tuning = self.library.contacts_tuning();
        p.wheels_on = wheels;
        self.report.contacts = first;
        self.report.wheels = wheels;
    }

    /// Retail's boot posts: c_emitter_utility → Start_up_Play_ctl (Common.abk) → c_foley_utility.
    fn boot_utilities(&mut self) {
        if let Ok(mut rt) = self.shared.lock() {
            if let Some(class) = rt.eval.class_id("c_emitter_utility") {
                rt.post(class, &[]);
            }
        }
        if self.ensure_bank(skate_audio::player::seams::UTILITY_BANK).is_ok() {
            if let Ok(mut rt) = self.shared.lock() {
                if let Some(class) = rt.eval.class_id(skate_audio::player::seams::UTILITY) {
                    rt.post(class, &[]);
                }
            }
        }
        if self.player.components && self.player.tricks_on {
            if let Ok(mut rt) = self.shared.lock() {
                if let Some(id) = rt.eval.class_id(skate_audio::player::tricks::FOLEY_UTILITY) {
                    rt.post(id, &[]);
                }
            }
        }
    }

    /// One rendered frame: upstream's `mixmap_frame`, `mixmap_tick`, `grain_bed::update`
    /// and `reverb_frame`, on the physics steps `game` took since the last call.
    /// `overstep`: the fixed step's overstep fraction (Class_Seams' rendered wheels).
    pub fn frame(&mut self, cues: &mut skate_events::Cues, camera: Option<Camera>, dt: f32, overstep: f32) {
        let Self { shared, library, mixmap: m, player, bed, cadence, holds, .. } = self;
        if let Some(bed) = bed.as_mut() {
            bed.slew_calls = Some(0);
        }
        let now = cues.riding.audio.wheel_position;
        player.seam_alpha = Some(overstep);
        player.step_wheels(cues.riding.wheels_before, now);
        if let Ok(mut rt) = shared.lock() {
            player.seam_frame(m, &cues.riding.audio, dt, &mut rt);
            if rt.mixer.buses.env.enabled() {
                let cam = camera.map(|c| skate_audio::bus::zones::Camera { position: c.position, forward: c.forward });
                rt.mixer.buses.env.update(dt.clamp(0.0, 0.25), skate_audio::bus::env::DEFAULT_PRESET, &[], cam.as_ref());
            }
        }
        let steps = cues.take_steps();
        if steps == 0 {
            return;
        }
        let ticks = steps.min(MAX_STEPS_PER_FRAME) as usize;
        let calls = cadence.advance(ticks);
        for id in 1..=4 {
            m.set_input(keys::MASTER, id, 32767);
        }
        for id in [1, 2, 5] {
            m.set_input(keys::MUSIC, id, 32767);
        }
        let reverb = shared.lock().ok().map(|r| r.mixer.buses.env.reverb_inputs());
        match reverb {
            Some(v) => {
                for (id, x) in v.into_iter().enumerate() {
                    m.set_input(keys::REVERB, id, x);
                }
            }
            None => m.set_input(keys::REVERB, 5, 32767),
        }
        m.set_input(keys::PAUSE, 0, 0);
        if let Some(bed) = bed.as_mut() {
            bed.slew_calls = Some(calls);
        }
        if !*holds {
            for (key, id) in [(keys::contacts(0), 1), (keys::contacts(0), 6), (keys::rail(0), 1), (keys::skateboard(0), 0), (keys::skateboard(0), 4), (keys::cracks(0), 0)] {
                m.hold_input(key, id);
            }
            *holds = true;
        }
        if calls > 0 {
            let jitter = player.eq_jitter();
            if let Ok(mut rt) = shared.lock() {
                rt.mixer.buses.eq.clear(jitter);
            }
        }
        let s = cues.riding.audio;
        let mix_dt = ticks as f32 * MIX_STEP;
        let listener = camera.map(|c| player.listener(c.position, c.forward, mix_dt, &s));
        player.jitter_steps = Some(calls);
        player.teleport_effect = None;
        player.write_inputs(m, &s, listener.as_ref());
        let routing = player.components && player.rolling_on;
        if let Some(bed) = bed.as_mut() {
            bed.write_inputs(m, &s, !routing);
        }
        let speed_scale = bed.as_ref().and_then(|b| b.push_scale());
        let loose = PlayerAudio::loose_board(&s, &cues.riding);
        if let Ok(mut rt) = shared.lock() {
            player.process(m, &s, &mut rt, speed_scale, loose);
        }
        for _ in 0..calls {
            m.tick(skate_audio::mixmap::cadence::CONSOLE_DT);
        }
        if let Ok(mut rt) = shared.lock() {
            player.update(m, &s, &mut rt, speed_scale, loose);
            rt.mixer.buses.flange.frame(std::array::from_fn(|i| m.level(keys::REVERB, i)));
            if reverb.is_some() {
                rt.mixer.buses.env.scale_frame(m.level(keys::REVERB, 4));
            }
        }
        if let Some(bed) = bed.as_mut() {
            let routed = (player.components && player.rolling_on).then(|| (std::mem::take(&mut player.routed.grains), player.routed.primary));
            grain_bed::step(bed, library, m, shared, &cues.riding, mix_dt, &player.tuning, routed);
        }
    }
}
