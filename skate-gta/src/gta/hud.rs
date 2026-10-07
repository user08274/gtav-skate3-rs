//! Draws the Skate 3 scoring HUD with ScriptHookV screen textures. Each
//! distinct texture region becomes a PNG in `SkateGTA\hud` next to the .asi.
use super::{log, module_dir, natives as n, shv};
use crate::hud_draw::{crop, place, SpriteKey};
use skate_gameplay::hud::{Hud, Sprite};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// Draw level above everything else this script draws.
const LEVEL: i32 = 1000;
/// How long an instance stays without a new call; drawn every frame anyway.
const TIME_MS: i32 = 100;
/// ScriptHookV's limit of instances per texture.
const INSTANCES: usize = 64;

/// Shared by all sessions: ScriptHookV textures live until scripts reload.
pub static PAINTER: std::sync::Mutex<Option<HudPainter>> = std::sync::Mutex::new(None);

pub fn with_painter(f: impl FnOnce(&mut HudPainter)) {
    let mut painter = PAINTER.lock().unwrap_or_else(|e| e.into_inner());
    f(painter.get_or_insert_with(HudPainter::default));
}

#[derive(Default)]
pub struct HudPainter {
    textures: HashMap<SpriteKey, i32>,
    atlases: HashMap<String, Vec<u8>>,
    /// Instances drawn last frame, per texture id, to hide the stale ones.
    drawn: HashMap<i32, usize>,
    failed: bool,
}

impl HudPainter {
    pub fn draw(&mut self, hud: Option<&Hud>, sprites: &[Sprite]) {
        let mut counts: HashMap<i32, usize> = HashMap::new();
        if let (false, Some(hud)) = (self.failed, hud) {
            let aspect = n::get_aspect_ratio();
            for (level, sprite) in sprites.iter().enumerate() {
                let id = match self.texture(hud, sprite) {
                    Ok(id) => id,
                    Err(error) => {
                        log(&format!("Skate 3 HUD textures: {error}"));
                        self.failed = true;
                        break;
                    }
                };
                let instance = counts.entry(id).or_default();
                if *instance >= INSTANCES {
                    continue;
                }
                let p = place(sprite, aspect);
                shv::draw_texture(&shv::TextureDraw {
                    id,
                    instance: *instance as i32,
                    level: LEVEL + level as i32,
                    time_ms: TIME_MS,
                    size: p.size,
                    center: [0.5, 0.5],
                    position: p.position,
                    rotation: p.rotation,
                    aspect,
                    color: p.color,
                });
                *instance += 1;
            }
        }
        // Instances not redrawn this frame would linger; make them invisible.
        for (&id, &before) in &self.drawn {
            for instance in counts.get(&id).copied().unwrap_or(0)..before {
                shv::draw_texture(&shv::TextureDraw {
                    id,
                    instance: instance as i32,
                    level: LEVEL,
                    time_ms: 0,
                    size: [0.0; 2],
                    center: [0.5, 0.5],
                    position: [0.0; 2],
                    rotation: 0.0,
                    aspect: 1.0,
                    color: [0.0; 4],
                });
            }
        }
        self.drawn = counts;
    }

    pub fn clear(&mut self) {
        self.draw(None, &[]);
    }

    fn texture(&mut self, hud: &Hud, sprite: &Sprite) -> Result<i32, String> {
        let key = SpriteKey::of(sprite);
        if let Some(&id) = self.textures.get(&key) {
            return Ok(id);
        }
        let dir = cache_dir()?;
        let path = dir.join(key.file_name());
        if !path.is_file() {
            let size = hud.texture_size(&key.texture).ok_or("unknown HUD texture")?;
            if !self.atlases.contains_key(&key.texture) {
                self.atlases.insert(key.texture.clone(), hud.texture_rgba(&key.texture)?);
            }
            let (w, h, rgba) = crop(&self.atlases[&key.texture], size, &key)?;
            let temporary = path.with_extension("tmp");
            std::fs::write(&temporary, crate::png::encode(w, h, &rgba)).map_err(|e| format!("{}: {e}", temporary.display()))?;
            std::fs::rename(&temporary, &path).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        let id = shv::create_texture(&ascii_path(&path)?)?;
        self.textures.insert(key, id);
        Ok(id)
    }
}

fn cache_dir() -> Result<PathBuf, String> {
    let dir = module_dir().join("SkateGTA").join("hud");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(dir)
}

/// ScriptHookV takes an ANSI path; a GTA folder with e.g. Cyrillic letters is
/// passed by its 8.3 short name instead.
fn ascii_path(path: &Path) -> Result<PathBuf, String> {
    let text = path.to_string_lossy();
    if text.is_ascii() {
        return Ok(path.to_path_buf());
    }
    use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let mut buffer = vec![0u16; 1024];
    let len = unsafe { GetShortPathNameW(wide.as_ptr(), buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    let short = String::from_utf16_lossy(&buffer[..len.min(buffer.len())]);
    if len == 0 || len >= buffer.len() || !short.is_ascii() {
        return Err(format!("the GTA folder path has non-Latin letters and no short name: {text}"));
    }
    Ok(PathBuf::from(short))
}
