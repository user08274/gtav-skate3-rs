//! The Skate 3 skateboard from the player's converted `private/skater.glb`:
//! the deck, truck and wheel meshes, each vertex rigid on its board bone
//! (SKATEBOARD_ROOT, the trucks, the wheels) and coloured from its texture.
//! Placed every frame on the same bones the Skate 3 render pose moves, so
//! the model follows the physics board, the flips and the spinning wheels.
use crate::coords::GtaVec;
use serde_json::Value;

/// Board materials in the stock skater scene.
pub const MATERIALS: [&str; 3] = ["Retail_SkateBoard", "Retail_SkateTruck", "Retail_SkateWheel"];

#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    /// Index into `BoardModel::bones`.
    pub bone: usize,
    /// Position and normal in the bone's frame (skate units, metres).
    pub local: [f32; 3],
    pub normal: [f32; 3],
}

#[derive(Clone, Debug)]
pub struct BoardModel {
    pub bones: Vec<String>,
    pub vertices: Vec<Vertex>,
    pub triangles: Vec<[u32; 3]>,
    /// Texture colour at each triangle's centre.
    pub colors: Vec<[f32; 3]>,
}

/// Deck triangles are split until none covers more than this area (m²), so
/// the per-triangle colours carry the deck's grip and graphic; thin edge
/// strips stay whole.
const DECK_DETAIL: f32 = 0.0006;

fn area(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> f32 {
    let (u, v) = (std::array::from_fn::<f32, 3, _>(|k| b[k] - a[k]), std::array::from_fn::<f32, 3, _>(|k| c[k] - a[k]));
    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt()
}

fn mid(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|k| (a[k] + b[k]) * 0.5)
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// A triangle ready to draw: GTA-space corners, unit normal and colour.
#[derive(Clone, Copy, Debug)]
pub struct Face {
    pub corners: [GtaVec; 3],
    pub normal: GtaVec,
    pub color: [f32; 3],
}

struct Glb {
    json: Value,
    bin: Vec<u8>,
}

impl Glb {
    fn parse(bytes: &[u8]) -> Result<Self, String> {
        let word = |at: usize| bytes.get(at..at + 4).map(|w| u32::from_le_bytes(w.try_into().unwrap()));
        if bytes.get(..4) != Some(b"glTF") {
            return Err("not a binary glTF".into());
        }
        let json_len = word(12).ok_or("truncated glTF")? as usize;
        let json: Value = serde_json::from_slice(bytes.get(20..20 + json_len).ok_or("truncated glTF JSON")?).map_err(|e| e.to_string())?;
        let at = 20 + json_len;
        let bin_len = word(at).ok_or("glTF without binary chunk")? as usize;
        let bin = bytes.get(at + 8..at + 8 + bin_len).ok_or("truncated glTF binary")?.to_vec();
        Ok(Self { json, bin })
    }

    fn view(&self, index: usize) -> Result<(&[u8], usize), String> {
        let v = &self.json["bufferViews"][index];
        let offset = v["byteOffset"].as_u64().unwrap_or(0) as usize;
        let len = v["byteLength"].as_u64().ok_or("bufferView without length")? as usize;
        let stride = v["byteStride"].as_u64().unwrap_or(0) as usize;
        Ok((self.bin.get(offset..offset + len).ok_or("bufferView outside the buffer")?, stride))
    }

    /// Accessor elements as f32 (floats, or unsigned bytes/shorts/ints as numbers).
    fn read(&self, index: usize) -> Result<Vec<Vec<f32>>, String> {
        let a = &self.json["accessors"][index];
        let count = a["count"].as_u64().ok_or("accessor without count")? as usize;
        let width = match a["type"].as_str() {
            Some("SCALAR") => 1,
            Some("VEC2") => 2,
            Some("VEC3") => 3,
            Some("VEC4") => 4,
            Some("MAT4") => 16,
            other => return Err(format!("accessor type {other:?}")),
        };
        let (component, size) = match a["componentType"].as_u64() {
            Some(5126) => ('f', 4),
            Some(5121) => ('b', 1),
            Some(5123) => ('s', 2),
            Some(5125) => ('i', 4),
            other => return Err(format!("component type {other:?}")),
        };
        let (data, stride) = self.view(a["bufferView"].as_u64().ok_or("sparse accessors are not supported")? as usize)?;
        let offset = a["byteOffset"].as_u64().unwrap_or(0) as usize;
        let stride = if stride == 0 { width * size } else { stride };
        (0..count)
            .map(|i| {
                (0..width)
                    .map(|k| {
                        let at = offset + i * stride + k * size;
                        let b = data.get(at..at + size).ok_or("accessor outside its view")?;
                        Ok(match component {
                            'f' => f32::from_le_bytes(b.try_into().unwrap()),
                            'b' => b[0] as f32,
                            's' => u16::from_le_bytes(b.try_into().unwrap()) as f32,
                            _ => u32::from_le_bytes(b.try_into().unwrap()) as f32,
                        })
                    })
                    .collect()
            })
            .collect()
    }

    fn image(&self, texture: usize) -> Result<(u32, u32, Vec<u8>), String> {
        let source = self.json["textures"][texture]["source"].as_u64().ok_or("texture without image")? as usize;
        let view = self.json["images"][source]["bufferView"].as_u64().ok_or("image outside the binary chunk")? as usize;
        crate::png::decode(self.view(view)?.0)
    }
}

fn sample(image: &(u32, u32, Vec<u8>), u: f32, v: f32) -> [f32; 3] {
    let (w, h, px) = image;
    let x = ((u.rem_euclid(1.0) * *w as f32) as u32).min(w - 1);
    let y = ((v.rem_euclid(1.0) * *h as f32) as u32).min(h - 1);
    let p = ((y * w + x) * 4) as usize;
    [px[p] as f32 / 255.0, px[p + 1] as f32 / 255.0, px[p + 2] as f32 / 255.0]
}

impl BoardModel {
    pub fn load(glb: &[u8]) -> Result<Self, String> {
        let glb = Glb::parse(glb)?;
        let j = &glb.json;
        let skin = &j["skins"][0];
        let joints: Vec<usize> = skin["joints"].as_array().ok_or("skater has no skin")?.iter().filter_map(|v| v.as_u64()).map(|v| v as usize).collect();
        let joint_names: Vec<String> = joints.iter().map(|&n| j["nodes"][n]["name"].as_str().unwrap_or("").to_string()).collect();
        let inverse_binds = glb.read(skin["inverseBindMatrices"].as_u64().ok_or("skin without inverse binds")? as usize)?;
        let mut model = BoardModel { bones: Vec::new(), vertices: Vec::new(), triangles: Vec::new(), colors: Vec::new() };
        let primitives = j["meshes"][0]["primitives"].as_array().ok_or("skater has no mesh")?;
        for p in primitives {
            let material = &j["materials"][p["material"].as_u64().unwrap_or(u64::MAX) as usize];
            if !MATERIALS.contains(&material["name"].as_str().unwrap_or("")) {
                continue;
            }
            let pbr = &material["pbrMetallicRoughness"];
            let factor: Vec<f32> = pbr["baseColorFactor"].as_array().map_or(vec![1.0; 4], |f| f.iter().map(|x| x.as_f64().unwrap_or(1.0) as f32).collect());
            let image = pbr["baseColorTexture"]["index"].as_u64().map(|t| glb.image(t as usize)).transpose()?;
            let attr = |name: &str| p["attributes"][name].as_u64().ok_or(format!("board mesh without {name}")).map(|i| i as usize);
            let positions = glb.read(attr("POSITION")?)?;
            let normals = glb.read(attr("NORMAL")?)?;
            let uvs = glb.read(attr("TEXCOORD_0")?)?;
            let bone_ids = glb.read(attr("JOINTS_0")?)?;
            let weights = glb.read(attr("WEIGHTS_0")?)?;
            let indices = glb.read(p["indices"].as_u64().ok_or("board mesh without indices")? as usize)?;
            let detail = material["name"].as_str() == Some("Retail_SkateBoard");
            // (bone, local, normal, uv) per source vertex.
            let mut source = Vec::with_capacity(positions.len());
            for i in 0..positions.len() {
                let strongest = (0..4).max_by(|&a, &b| weights[i][a].total_cmp(&weights[i][b])).unwrap();
                let joint = bone_ids[i][strongest] as usize;
                let name = joint_names.get(joint).ok_or("vertex on an unknown joint")?;
                let bone = match model.bones.iter().position(|b| b == name) {
                    Some(b) => b,
                    None => {
                        model.bones.push(name.clone());
                        model.bones.len() - 1
                    }
                };
                // glTF matrices are column-major.
                let m = &inverse_binds[joint];
                let (q, n) = (&positions[i], &normals[i]);
                let local: [f32; 3] = std::array::from_fn(|r| m[r] * q[0] + m[4 + r] * q[1] + m[8 + r] * q[2] + m[12 + r]);
                let normal: [f32; 3] = std::array::from_fn(|r| m[r] * n[0] + m[4 + r] * n[1] + m[8 + r] * n[2]);
                source.push((bone, local, normal, [uvs[i][0], uvs[i][1], 0.0f32]));
            }
            let color_at = |uv: [f32; 3]| -> [f32; 3] {
                let texel = image.as_ref().map_or([1.0; 3], |img| sample(img, uv[0], uv[1]));
                std::array::from_fn(|k| texel[k] * factor[k])
            };
            for t in indices.chunks(3).filter(|t| t.len() == 3) {
                let corners = [0, 1, 2].map(|k| source[t[k][0] as usize]);
                let mut pending = vec![corners];
                while let Some(tri) = pending.pop() {
                    // Halve the longest edge until every edge is short: no slivers multiply.
                    let (edge, _) = (0..3)
                        .map(|k| (k, distance(tri[k].1, tri[(k + 1) % 3].1)))
                        .fold((0, 0.0), |a, b| if b.1 > a.1 { b } else { a });
                    let same_bone = tri[0].0 == tri[1].0 && tri[1].0 == tri[2].0;
                    if detail && same_bone && area(tri[0].1, tri[1].1, tri[2].1) > DECK_DETAIL {
                        let (a, b, c) = (tri[edge], tri[(edge + 1) % 3], tri[(edge + 2) % 3]);
                        let m = (a.0, mid(a.1, b.1), mid(a.2, b.2), mid(a.3, b.3));
                        pending.extend([[a, m, c], [m, b, c]]);
                        continue;
                    }
                    let base = model.vertices.len() as u32;
                    for (bone, local, normal, _) in tri {
                        model.vertices.push(Vertex { bone, local, normal });
                    }
                    let centre = std::array::from_fn(|k| (tri[0].3[k] + tri[1].3[k] + tri[2].3[k]) / 3.0);
                    model.triangles.push([base, base + 1, base + 2]);
                    model.colors.push(color_at(centre));
                }
            }
        }
        if model.triangles.is_empty() {
            return Err("skater.glb has no skateboard meshes".into());
        }
        Ok(model)
    }

    /// Merges vertices of the same bone closer than `cell` (metres) and drops
    /// collapsed triangles: fewer draw calls for the small truck and wheel
    /// parts, whose detail is sub-centimetre.
    pub fn simplified(&self, cell: f32, keep_bones: &[&str]) -> Self {
        let mut map = std::collections::HashMap::new();
        let mut vertices: Vec<Vertex> = Vec::new();
        let mut remap = Vec::with_capacity(self.vertices.len());
        for v in &self.vertices {
            let keep = keep_bones.contains(&self.bones[v.bone].as_str());
            let key = if keep {
                (v.bone, i32::MIN, i32::MIN, vertices.len() as i32)
            } else {
                let q = |x: f32| (x / cell).round() as i32;
                (v.bone, q(v.local[0]), q(v.local[1]), q(v.local[2]))
            };
            let index = *map.entry(key).or_insert_with(|| {
                vertices.push(*v);
                vertices.len() - 1
            });
            remap.push(index as u32);
        }
        let mut seen = std::collections::HashSet::new();
        let (mut triangles, mut colors) = (Vec::new(), Vec::new());
        for (t, color) in self.triangles.iter().zip(&self.colors) {
            let t = t.map(|i| remap[i as usize]);
            let mut key = t;
            key.sort_unstable();
            if t[0] != t[1] && t[1] != t[2] && t[0] != t[2] && seen.insert(key) {
                triangles.push(t);
                colors.push(*color);
            }
        }
        Self { bones: self.bones.clone(), vertices, triangles, colors }
    }

    /// Faces in GTA space for Skate 3 render-pose bone frames given as
    /// (origin, axes) in GTA space.
    pub fn place(&self, frame: impl Fn(&str) -> Option<(GtaVec, [GtaVec; 3])>) -> Vec<Face> {
        // The stock skater.glb went through Blender: its bone-local axes are
        // the native ones turned -90 degrees about X (the original host's
        // render_basis, columns X, -Z, Y).
        let frames: Vec<Option<(GtaVec, [GtaVec; 3])>> = self
            .bones
            .iter()
            .map(|b| frame(b).map(|(origin, a)| (origin, [a[0], a[2].scale(-1.0), a[1]])))
            .collect();
        let world: Vec<Option<(GtaVec, GtaVec)>> = self
            .vertices
            .iter()
            .map(|v| {
                let (origin, a) = frames[v.bone]?;
                let along = |l: [f32; 3]| a[0].scale(l[0]).add(a[1].scale(l[1])).add(a[2].scale(l[2]));
                Some((origin.add(along(v.local)), along(v.normal)))
            })
            .collect();
        self.triangles
            .iter()
            .zip(&self.colors)
            .filter_map(|(t, &color)| {
                let [a, b, c] = t.map(|i| world[i as usize]);
                let (a, b, c) = (a?, b?, c?);
                let normal = a.1.add(b.1).add(c.1).normalized();
                Some(Face { corners: [a.0, b.0, c.0], normal, color })
            })
            .collect()
    }
}
