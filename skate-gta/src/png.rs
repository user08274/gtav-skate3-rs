//! Minimal PNG writer (RGBA8, stored deflate blocks): ScriptHookV loads
//! HUD textures only from image files.

pub fn encode(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    assert_eq!(rgba.len(), width as usize * height as usize * 4, "RGBA size");
    let row = width as usize * 4;
    let mut raw = Vec::with_capacity((row + 1) * height as usize);
    for line in rgba.chunks(row.max(1)).take(height as usize) {
        raw.push(0); // filter: none
        raw.extend_from_slice(line);
    }
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::new();
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA, no interlace
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &zlib_stored(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}

/// Decodes an 8-bit, non-interlaced greyscale, RGB or RGBA PNG to RGBA.
pub fn decode(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    if bytes.len() < 8 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return Err("not a PNG".into());
    }
    let (mut at, mut header, mut data) = (8usize, None, Vec::new());
    while at + 8 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        let kind = &bytes[at + 4..at + 8];
        let body = bytes.get(at + 8..at + 8 + len).ok_or("truncated PNG chunk")?;
        match kind {
            b"IHDR" => header = Some((u32::from_be_bytes(body[0..4].try_into().unwrap()), u32::from_be_bytes(body[4..8].try_into().unwrap()), body[8], body[9], body[12])),
            b"IDAT" => data.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        at += 12 + len;
    }
    let (w, h, depth, color, interlace) = header.ok_or("PNG without IHDR")?;
    let channels = match color {
        0 => 1,
        2 => 3,
        6 => 4,
        _ => return Err(format!("PNG colour type {color} not supported")),
    };
    if depth != 8 || interlace != 0 {
        return Err("only 8-bit, non-interlaced PNGs are supported".into());
    }
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib(&data).map_err(|e| format!("PNG data: {e:?}"))?;
    let stride = w as usize * channels;
    if raw.len() < (stride + 1) * h as usize {
        return Err("PNG data too short".into());
    }
    let mut pixels = vec![0u8; stride * h as usize];
    for y in 0..h as usize {
        let filter = raw[y * (stride + 1)];
        let line = &raw[y * (stride + 1) + 1..(y + 1) * (stride + 1)];
        for x in 0..stride {
            let a = if x >= channels { pixels[y * stride + x - channels] as i32 } else { 0 };
            let b = if y > 0 { pixels[(y - 1) * stride + x] as i32 } else { 0 };
            let c = if x >= channels && y > 0 { pixels[(y - 1) * stride + x - channels] as i32 } else { 0 };
            let predict = match filter {
                0 => 0,
                1 => a,
                2 => b,
                3 => (a + b) / 2,
                4 => {
                    let p = a + b - c;
                    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
                    if pa <= pb && pa <= pc { a } else if pb <= pc { b } else { c }
                }
                _ => return Err(format!("PNG filter {filter}")),
            };
            pixels[y * stride + x] = (line[x] as i32 + predict) as u8;
        }
    }
    let rgba = pixels
        .chunks(channels)
        .flat_map(|p| match channels {
            1 => [p[0], p[0], p[0], 255],
            3 => [p[0], p[1], p[2], 255],
            _ => [p[0], p[1], p[2], p[3]],
        })
        .collect();
    Ok((w, h, rgba))
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = if data.is_empty() { vec![&[]] } else { data.chunks(0xFFFF).collect() };
    for (i, block) in blocks.iter().enumerate() {
        out.push((i + 1 == blocks.len()) as u8);
        let len = block.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    out.extend_from_slice(&((b << 16) | a).to_be_bytes());
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_reads_back_what_encode_wrote() {
        let rgba: Vec<u8> = (0..5 * 3 * 4).map(|i| (i * 37 % 251) as u8).collect();
        let (w, h, back) = decode(&encode(5, 3, &rgba)).unwrap();
        assert_eq!((w, h), (5, 3));
        assert_eq!(back, rgba);
    }

    #[test]
    fn crc_matches_the_png_reference() {
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
    }

    #[test]
    fn one_pixel_image_is_well_formed() {
        let png = encode(1, 1, &[255, 0, 0, 255]);
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
        // IDAT payload: zlib header, one final stored block of 5 bytes, adler.
        let idat = &png[33 + 8..];
        assert_eq!(&idat[..8], &[0x78, 0x01, 1, 5, 0, 0xFA, 0xFF, 0]);
    }
}
