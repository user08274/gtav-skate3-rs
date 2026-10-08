//! The original Skate 3 scoring HUD (APT movie and its ActionScript) driven by
//! the gameplay's scoring, flattened into sprites a host can draw. Geometry is
//! on the movie's 1280x720 canvas, y down.
use crate::{apt_scene, host::Game, hud_runtime};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub const CANVAS: [f32; 2] = [1280.0, 720.0];

/// One textured rectangle: a region of a texture placed by centre, size and
/// rotation. `mirrored` means the region must be flipped left-right.
#[derive(Clone, Debug, PartialEq)]
pub struct Sprite {
    pub texture: String,
    /// Pixel rectangle in the texture: x0, y0, x1, y1.
    pub region: [u32; 4],
    pub mirrored: bool,
    pub center: [f32; 2],
    pub size: [f32; 2],
    pub rotation_degrees: f32,
    pub color: [f32; 4],
}

pub struct Hud {
    root: PathBuf,
    runtime: hud_runtime::Runtime,
    shapes: apt_scene::Shapes,
    sizes: BTreeMap<String, [u32; 2]>,
}

impl Hud {
    /// `assets` is the converted assets folder; the HUD lives in `private/hud`.
    pub fn load(assets: &Path, game: &Game) -> Result<Self, String> {
        let root = assets.join("private/hud");
        let path = root.join("runtime/trickdisplay.json");
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let source: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if source["format"] != "skate3-scoring-hud" {
            return Err("trickdisplay.json is not a Skate 3 scoring HUD".into());
        }
        let runtime = hud_runtime::Runtime::load(&source, game.scoring().hud_input())?;
        let shapes: apt_scene::Shapes = serde_json::from_value(source["shapes"].clone()).map_err(|e| e.to_string())?;
        let mut sizes = BTreeMap::new();
        for shape in shapes.values().flatten() {
            sizes.insert(shape.texture.rgba.clone(), [shape.texture.width, shape.texture.height]);
        }
        for font in runtime.bindings.movie.text_assets.fonts.values() {
            sizes.insert(font.texture.clone(), font.size);
        }
        for (file, size) in &sizes {
            let len = std::fs::metadata(root.join(file)).map_err(|e| format!("HUD {file}: {e}"))?.len();
            if len != size[0] as u64 * size[1] as u64 * 4 {
                return Err(format!("HUD texture {file} has the wrong size"));
            }
        }
        Ok(Self { root, runtime, shapes, sizes })
    }

    pub fn update(&mut self, game: &Game) -> Result<(), String> {
        let scoring = game.scoring();
        self.runtime.update(scoring.hud_input(), scoring.new_trick, scoring.modified_trick, scoring.close_tricks)
    }

    pub fn texture_size(&self, file: &str) -> Option<[u32; 2]> {
        self.sizes.get(file).copied()
    }

    pub fn texture_rgba(&self, file: &str) -> Result<Vec<u8>, String> {
        if !self.sizes.contains_key(file) {
            return Err(format!("unknown HUD texture {file}"));
        }
        std::fs::read(self.root.join(file)).map_err(|e| format!("HUD {file}: {e}"))
    }

    pub fn sprites(&self) -> Result<Vec<Sprite>, String> {
        let draws = apt_scene::draw(&self.runtime.bindings.movie, &self.runtime.vm, &self.shapes)?;
        Ok(draws.iter().flat_map(|d| self.texture_size(&d.texture).map(|size| sprites_of(d, size)).unwrap_or_default()).collect())
    }
}

/// Shapes and text glyphs are quads of two triangles (six vertices) each.
pub fn sprites_of(draw: &apt_scene::Draw, size: [u32; 2]) -> Vec<Sprite> {
    if draw.vertices.len() % 6 != 0 {
        return sprite(draw, &draw.vertices, size).into_iter().collect();
    }
    draw.vertices.chunks(6).filter_map(|quad| sprite(draw, quad, size)).collect()
}

/// Recover the rectangle a quad draws.
fn sprite(draw: &apt_scene::Draw, vertices: &[apt_scene::Vertex], size: [u32; 2]) -> Option<Sprite> {
    let mut corners: Vec<([f32; 2], [f32; 2])> = Vec::new();
    for v in vertices {
        if !corners.iter().any(|(p, uv)| close(*p, v.position) && close(*uv, v.uv)) {
            corners.push((v.position, v.uv));
        }
    }
    if corners.len() != 4 {
        return None;
    }
    let (mut u0, mut v0, mut u1, mut v1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (_, uv) in &corners {
        u0 = u0.min(uv[0]);
        v0 = v0.min(uv[1]);
        u1 = u1.max(uv[0]);
        v1 = v1.max(uv[1]);
    }
    let at = |u: f32, v: f32| corners.iter().find(|(_, uv)| close(*uv, [u, v])).map(|(p, _)| *p);
    let (p00, p10, p01) = (at(u0, v0)?, at(u1, v0)?, at(u0, v1)?);
    let w = [p10[0] - p00[0], p10[1] - p00[1]];
    let h = [p01[0] - p00[0], p01[1] - p00[1]];
    let (width, height) = (w[0].hypot(w[1]), h[0].hypot(h[1]));
    if width < 0.01 || height < 0.01 {
        return None;
    }
    // Canvas y points down, so an unmirrored image has w x h > 0.
    let mirrored = w[0] * h[1] - w[1] * h[0] < 0.0;
    let across = if mirrored { [-w[0], -w[1]] } else { w };
    let pixel = |t: f32, n: u32| ((t.clamp(0.0, 1.0) * n as f32).round() as u32).min(n);
    let region = [pixel(u0, size[0]), pixel(v0, size[1]), pixel(u1, size[0]), pixel(v1, size[1])];
    if region[2] <= region[0] || region[3] <= region[1] {
        return None;
    }
    let color = std::array::from_fn(|i| (draw.multiply[i] + draw.add[i]).clamp(0.0, 1.0));
    if color[3] <= 0.0 {
        return None;
    }
    Some(Sprite {
        texture: draw.texture.clone(),
        region,
        mirrored,
        center: [p00[0] + (w[0] + h[0]) * 0.5, p00[1] + (w[1] + h[1]) * 0.5],
        size: [width, height],
        rotation_degrees: across[1].atan2(across[0]).to_degrees(),
        color,
    })
}

fn close(a: [f32; 2], b: [f32; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4
}
