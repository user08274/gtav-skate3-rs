//! Places Skate 3 HUD sprites (1280x720 canvas) on the GTA screen through
//! ScriptHookV's drawTexture, and cuts their texture regions into images.
use skate_gameplay::hud::{Sprite, CANVAS};

/// One image file per distinct texture region.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpriteKey {
    pub texture: String,
    pub region: [u32; 4],
    pub mirrored: bool,
}

impl SpriteKey {
    pub fn of(sprite: &Sprite) -> Self {
        Self { texture: sprite.texture.clone(), region: sprite.region, mirrored: sprite.mirrored }
    }

    pub fn file_name(&self) -> String {
        let text = format!("{}|{:?}|{}", self.texture, self.region, self.mirrored);
        format!("{:08x}{:08x}.png", crate::hash::joaat(&text), crate::hash::joaat(&text.chars().rev().collect::<String>()))
    }
}

/// drawTexture arguments for one sprite.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// Width and height, both in screen widths (ScriptHookV convention).
    pub size: [f32; 2],
    /// Centre in normalized screen coordinates, y down.
    pub position: [f32; 2],
    /// Clockwise turns.
    pub rotation: f32,
    pub color: [f32; 4],
}

/// The canvas keeps its 16:9 shape: on wider or narrower screens it is
/// centred horizontally with square pixels.
pub fn place(sprite: &Sprite, aspect: f32) -> Placement {
    let aspect = if aspect.is_finite() && aspect > 0.1 { aspect } else { 16.0 / 9.0 };
    let squeeze = (CANVAS[0] / CANVAS[1]) / aspect;
    Placement {
        size: [sprite.size[0] / CANVAS[1] / aspect, sprite.size[1] / CANVAS[1] / aspect],
        position: [0.5 + (sprite.center[0] / CANVAS[0] - 0.5) * squeeze, sprite.center[1] / CANVAS[1]],
        rotation: sprite.rotation_degrees / 360.0,
        color: sprite.color,
    }
}

/// The region's pixels, flipped left-right when the sprite is mirrored.
pub fn crop(rgba: &[u8], size: [u32; 2], key: &SpriteKey) -> Result<(u32, u32, Vec<u8>), String> {
    let [x0, y0, x1, y1] = key.region;
    if x1 > size[0] || y1 > size[1] || x0 >= x1 || y0 >= y1 || rgba.len() != size[0] as usize * size[1] as usize * 4 {
        return Err(format!("HUD region {:?} outside {}", key.region, key.texture));
    }
    let (w, h) = (x1 - x0, y1 - y0);
    let mut out = Vec::with_capacity(w as usize * h as usize * 4);
    for y in y0..y1 {
        for i in 0..w {
            let x = if key.mirrored { x1 - 1 - i } else { x0 + i };
            let p = (y as usize * size[0] as usize + x as usize) * 4;
            out.extend_from_slice(&rgba[p..p + 4]);
        }
    }
    Ok((w, h, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sprite(center: [f32; 2], size: [f32; 2]) -> Sprite {
        Sprite {
            texture: "t".into(),
            region: [0, 0, 2, 1],
            mirrored: false,
            center,
            size,
            rotation_degrees: 90.0,
            color: [1.0; 4],
        }
    }

    #[test]
    fn widescreen_canvas_matches_the_screen() {
        let p = place(&sprite([320.0, 360.0], [128.0, 72.0]), 16.0 / 9.0);
        assert!((p.position[0] - 0.25).abs() < 1e-6 && (p.position[1] - 0.5).abs() < 1e-6);
        assert!((p.size[0] - 0.1).abs() < 1e-6 && (p.size[1] - 0.05625).abs() < 1e-6);
        assert!((p.rotation - 0.25).abs() < 1e-6);
    }

    #[test]
    fn ultrawide_keeps_square_pixels_centred() {
        let aspect = 21.0 / 9.0;
        let p = place(&sprite([0.0, 0.0], [72.0, 72.0]), aspect);
        // Equal pixel width and height: both sizes are in screen widths.
        assert_eq!(p.size[0], p.size[1]);
        let edge = 0.5 - 0.5 * (16.0 / 9.0) / aspect;
        assert!((p.position[0] - edge).abs() < 1e-6);
    }

    #[test]
    fn crop_flips_mirrored_regions() {
        let rgba: Vec<u8> = (0..4 * 3 * 2).map(|i| i as u8).collect();
        let key = SpriteKey { texture: "t".into(), region: [1, 0, 3, 2], mirrored: true };
        let (w, h, px) = crop(&rgba, [3, 2], &key).unwrap();
        assert_eq!((w, h), (2, 2));
        assert_eq!(&px[..8], &[8, 9, 10, 11, 4, 5, 6, 7]);
        assert!(crop(&rgba, [3, 2], &SpriteKey { region: [0, 0, 4, 1], ..key }).is_err());
    }
}
