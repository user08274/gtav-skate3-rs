//! Retargets the Skate 3 skater pose onto a GTA ped skeleton.
//!
//! Only joint positions are taken from Skate 3. Every mapped GTA bone keeps its
//! own length and rest orientation and is turned by the smallest rotation that
//! points it at the matching Skate 3 joint; the pelvis also takes its facing
//! from the hips and thighs. Unmapped bones (fingers, face, roll and IK helper
//! bones) ride along rigidly with their nearest mapped ancestor.
use crate::coords::GtaVec;

/// Row-vector 3x3 rotation (rows are the bone's X, Y, Z axes) and position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BonePose {
    pub axes: [GtaVec; 3],
    pub position: GtaVec,
}

/// GTA bone tags (eAnimBoneTag) and the Skate 3 joints driving them.
pub struct Link {
    pub tag: i32,
    /// Skate 3 joint at this bone and the joint it points to.
    pub from: &'static str,
    pub to: &'static str,
    /// GTA bone tag whose rest position this bone points to.
    pub toward: i32,
}

pub const PELVIS: i32 = 0x2E28;
pub const SPINE0: i32 = 0x5C01;
pub const L_THIGH: i32 = 0xE39F;
pub const R_THIGH: i32 = 0xCA72;

pub const LINKS: [Link; 17] = [
    Link { tag: SPINE0, from: "SPINE", to: "SPINE1", toward: 0x60F0 },
    Link { tag: 0x60F0, from: "SPINE1", to: "SPINE2", toward: 0x60F1 },
    Link { tag: 0x60F1, from: "SPINE2", to: "SPINE3", toward: 0x60F2 },
    Link { tag: 0x60F2, from: "SPINE3", to: "NECK", toward: 0x9995 },
    Link { tag: 0x9995, from: "NECK", to: "HEAD", toward: 0x796E },
    Link { tag: 0xFCD9, from: "LEFTSHOULDER", to: "LEFTARM", toward: 0xB1C5 },
    Link { tag: 0xB1C5, from: "LEFTARM", to: "LEFTFOREARM", toward: 0xEEEB },
    Link { tag: 0xEEEB, from: "LEFTFOREARM", to: "LEFTHAND", toward: 0x49D9 },
    Link { tag: 0x29D2, from: "RIGHTSHOULDER", to: "RIGHTARM", toward: 0x9D4D },
    Link { tag: 0x9D4D, from: "RIGHTARM", to: "RIGHTFOREARM", toward: 0x6E5C },
    Link { tag: 0x6E5C, from: "RIGHTFOREARM", to: "RIGHTHAND", toward: 0xDEAD },
    Link { tag: L_THIGH, from: "LEFTUPLEG", to: "LEFTLEG", toward: 0xF9BB },
    Link { tag: 0xF9BB, from: "LEFTLEG", to: "LEFTFOOT", toward: 0x3779 },
    Link { tag: 0x3779, from: "LEFTFOOT", to: "LEFTTOEBASE", toward: 0x083C },
    Link { tag: R_THIGH, from: "RIGHTUPLEG", to: "RIGHTLEG", toward: 0x9000 },
    Link { tag: 0x9000, from: "RIGHTLEG", to: "RIGHTFOOT", toward: 0xCC4D },
    Link { tag: 0xCC4D, from: "RIGHTFOOT", to: "RIGHTTOEBASE", toward: 0x512D },
];

/// Rotation as a 3x3 matrix with column vectors, applied as `m * v`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Rot([GtaVec; 3]);

impl Rot {
    const IDENTITY: Self = Self([GtaVec::new(1.0, 0.0, 0.0), GtaVec::new(0.0, 1.0, 0.0), GtaVec::new(0.0, 0.0, 1.0)]);

    fn apply(&self, v: GtaVec) -> GtaVec {
        self.0[0].scale(v.x).add(self.0[1].scale(v.y)).add(self.0[2].scale(v.z))
    }

    fn then(&self, first: &Rot) -> Rot {
        Rot(first.0.map(|c| self.apply(c)))
    }

    /// From orthonormal columns `to` and `from`: the rotation taking `from` onto `to`.
    fn between_bases(to: [GtaVec; 3], from: [GtaVec; 3]) -> Rot {
        // to * from^T, with from^T applied to the unit axes.
        let rows = |i: usize| GtaVec::new(component(from[0], i), component(from[1], i), component(from[2], i));
        Rot([0, 1, 2].map(|i| {
            let r = rows(i);
            to[0].scale(r.x).add(to[1].scale(r.y)).add(to[2].scale(r.z))
        }))
    }

    /// Smallest rotation taking unit `a` onto unit `b`.
    fn from_to(a: GtaVec, b: GtaVec) -> Rot {
        let c = dot(a, b);
        let v = cross(a, b);
        if c < -0.9999 {
            let axis = if a.x.abs() < 0.9 { cross(a, GtaVec::new(1.0, 0.0, 0.0)) } else { cross(a, GtaVec::new(0.0, 1.0, 0.0)) }.normalized();
            return Rot::axis_angle(axis, std::f32::consts::PI);
        }
        let k = 1.0 / (1.0 + c);
        Rot([
            GtaVec::new(v.x * v.x * k + c, v.x * v.y * k + v.z, v.x * v.z * k - v.y),
            GtaVec::new(v.y * v.x * k - v.z, v.y * v.y * k + c, v.y * v.z * k + v.x),
            GtaVec::new(v.z * v.x * k + v.y, v.z * v.y * k - v.x, v.z * v.z * k + c),
        ])
    }

    fn axis_angle(axis: GtaVec, angle: f32) -> Rot {
        let (s, c) = angle.sin_cos();
        let t = 1.0 - c;
        let (x, y, z) = (axis.x, axis.y, axis.z);
        Rot([
            GtaVec::new(t * x * x + c, t * x * y + s * z, t * x * z - s * y),
            GtaVec::new(t * x * y - s * z, t * y * y + c, t * y * z + s * x),
            GtaVec::new(t * x * z + s * y, t * y * z - s * x, t * z * z + c),
        ])
    }
}

fn component(v: GtaVec, i: usize) -> f32 {
    [v.x, v.y, v.z][i]
}
fn dot(a: GtaVec, b: GtaVec) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z
}
fn cross(a: GtaVec, b: GtaVec) -> GtaVec {
    GtaVec::new(a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x)
}
fn length(v: GtaVec) -> f32 {
    dot(v, v).sqrt()
}

/// Orthonormal frame from a primary direction and a secondary hint.
fn frame(up: GtaVec, side: GtaVec) -> Option<[GtaVec; 3]> {
    let z = up.normalized();
    let y = cross(z, side);
    if length(y) < 1e-4 || length(up) < 1e-4 {
        return None;
    }
    let y = y.normalized();
    let x = cross(y, z);
    Some([x, y, z])
}

pub struct Rig {
    rest: Vec<BonePose>,
    parents: Vec<i32>,
    /// Bone index of each LINKS entry and of its `toward` bone.
    links: Vec<Option<(usize, usize)>>,
    pelvis: usize,
    spine0: usize,
    thighs: (usize, usize),
    order: Vec<usize>,
}

impl Rig {
    /// `rest` are model-space bone matrices, `parents` the skeleton hierarchy and
    /// `index_of` resolves a bone tag to its index.
    pub fn new(rest: Vec<BonePose>, parents: Vec<i32>, index_of: impl Fn(i32) -> Option<usize>) -> Result<Self, String> {
        if rest.len() != parents.len() || rest.is_empty() {
            return Err("ped skeleton is empty".into());
        }
        let need = |tag: i32| index_of(tag).filter(|&i| i < rest.len()).ok_or(format!("ped bone {tag:#x} not found"));
        let pelvis = need(PELVIS)?;
        let spine0 = need(SPINE0)?;
        let thighs = (need(L_THIGH)?, need(R_THIGH)?);
        let links = LINKS
            .iter()
            .map(|l| Some((index_of(l.tag)?, index_of(l.toward)?)).filter(|(a, b)| *a < rest.len() && *b < rest.len()))
            .collect();
        // Parents before children, whatever order the skeleton stores bones in.
        let mut order = Vec::with_capacity(rest.len());
        let mut placed = vec![false; rest.len()];
        while order.len() < rest.len() {
            let before = order.len();
            for i in 0..rest.len() {
                let p = parents[i];
                if !placed[i] && (p < 0 || p as usize >= rest.len() || placed[p as usize]) {
                    placed[i] = true;
                    order.push(i);
                }
            }
            if order.len() == before {
                return Err("ped skeleton hierarchy has a cycle".into());
            }
        }
        Ok(Self { rest, parents, links, pelvis, spine0, thighs, order })
    }

    pub fn bone_count(&self) -> usize {
        self.rest.len()
    }

    /// Diagnostic summary: resolved bones and a few rest positions.
    pub fn describe(&self) -> String {
        let p = |i: usize| self.rest[i].position;
        let links: Vec<_> = LINKS
            .iter()
            .zip(&self.links)
            .map(|(l, r)| format!("{}:{}", l.from, r.map_or("-".into(), |(a, b)| format!("{a}>{b}"))))
            .collect();
        format!(
            "bones {} pelvis {} ({:?}) spine0 {} ({:?}) thighs {:?} links [{}] pelvis axes {:?}",
            self.rest.len(),
            self.pelvis,
            p(self.pelvis),
            self.spine0,
            p(self.spine0),
            (p(self.thighs.0), p(self.thighs.1)),
            links.join(" "),
            self.rest[self.pelvis].axes
        )
    }

    pub fn rest(&self) -> &[BonePose] {
        &self.rest
    }

    /// Model-space pose for every bone. `joint` gives Skate 3 joint positions
    /// already in the ped's model space; missing joints leave bones at rest.
    pub fn solve(&self, joint: impl Fn(&str) -> Option<GtaVec>) -> Vec<BonePose> {
        let n = self.rest.len();
        let mut delta = vec![Rot::IDENTITY; n];
        let mut position: Vec<GtaVec> = self.rest.iter().map(|b| b.position).collect();
        let link_of = |bone: usize| self.links.iter().zip(LINKS.iter()).find(|(l, _)| l.is_some_and(|(b, _)| b == bone));
        for &b in &self.order {
            let parent = self.parents[b];
            let (parent_delta, parent_pos, parent_rest) = if parent >= 0 && (parent as usize) < n {
                let p = parent as usize;
                (delta[p], position[p], self.rest[p].position)
            } else {
                (Rot::IDENTITY, GtaVec::default(), GtaVec::default())
            };
            let carried = parent_pos.add(parent_delta.apply(self.rest[b].position.sub(parent_rest)));
            if b == self.pelvis {
                let rest_frame = frame(
                    self.rest[self.spine0].position.sub(self.rest[b].position),
                    self.rest[self.thighs.0].position.sub(self.rest[self.thighs.1].position),
                );
                let target = match (joint("HIPS"), joint("SPINE"), joint("LEFTUPLEG"), joint("RIGHTUPLEG")) {
                    (Some(h), Some(s), Some(l), Some(r)) => frame(s.sub(h), l.sub(r)).map(|f| (f, h)),
                    _ => None,
                };
                match (rest_frame, target) {
                    (Some(from), Some((to, hips))) => {
                        delta[b] = Rot::between_bases(to, from);
                        position[b] = hips;
                    }
                    _ => {
                        delta[b] = parent_delta;
                        position[b] = carried;
                    }
                }
                continue;
            }
            position[b] = carried;
            delta[b] = parent_delta;
            if let Some((Some((_, toward)), link)) = link_of(b) {
                let rest_dir = self.rest[*toward].position.sub(self.rest[b].position);
                if let (Some(a), Some(c)) = (joint(link.from), joint(link.to)) {
                    let target = c.sub(a);
                    if length(rest_dir) > 1e-4 && length(target) > 1e-4 {
                        let current = parent_delta.apply(rest_dir).normalized();
                        delta[b] = Rot::from_to(current, target.normalized()).then(&parent_delta);
                    }
                }
            }
        }
        (0..n)
            .map(|b| BonePose { axes: self.rest[b].axes.map(|a| delta[b].apply(a)), position: position[b] })
            .collect()
    }
}

/// Parent-relative matrices of an entity-relative pose: what GTA keeps as a
/// skeleton's "locals" and turns back into the entity-relative ones. Axes
/// are taken as orthonormal (no scale), roots stay as they are.
pub fn to_locals(pose: &[BonePose], parents: &[i32]) -> Vec<BonePose> {
    pose.iter()
        .enumerate()
        .map(|(i, bone)| match parents.get(i).copied().filter(|&p| p >= 0 && (p as usize) < pose.len()) {
            None => *bone,
            Some(p) => {
                let parent = &pose[p as usize];
                let local = |v: GtaVec| GtaVec::new(dot(v, parent.axes[0]), dot(v, parent.axes[1]), dot(v, parent.axes[2]));
                BonePose { axes: bone.axes.map(local), position: local(bone.position.sub(parent.position)) }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locals_compose_back_into_the_pose() {
        // Root turned 90 degrees about Z; child 1 m along the root's X, tilted.
        let (s, c) = (0.3f32.sin(), 0.3f32.cos());
        let root = BonePose { axes: [v(0., 1., 0.), v(-1., 0., 0.), v(0., 0., 1.)], position: v(1., 2., 3.) };
        let child = BonePose { axes: [v(0., c, s), v(-1., 0., 0.), v(0., -s, c)], position: v(1., 3., 3.) };
        let locals = to_locals(&[root, child], &[-1, 0]);
        assert_eq!(locals[0].position, root.position);
        // Child sits 1 m along the parent's own X axis.
        let p = locals[1].position;
        assert!((p.x - 1.0).abs() < 1e-6 && p.y.abs() < 1e-6 && p.z.abs() < 1e-6, "{p:?}");
        // Composing parent * local gives the child back.
        let compose = |l: GtaVec| root.axes[0].scale(l.x).add(root.axes[1].scale(l.y)).add(root.axes[2].scale(l.z));
        for k in 0..3 {
            let back = compose(locals[1].axes[k]);
            assert!(back.sub(child.axes[k]).x.abs() < 1e-6 && back.sub(child.axes[k]).y.abs() < 1e-6 && back.sub(child.axes[k]).z.abs() < 1e-6);
        }
        let back = compose(locals[1].position).add(root.position);
        assert!(back.sub(child.position).x.abs() < 1e-6 && back.sub(child.position).y.abs() < 1e-6);
    }

    fn v(x: f32, y: f32, z: f32) -> GtaVec {
        GtaVec::new(x, y, z)
    }

    /// A small GTA-like skeleton: root, pelvis, spine chain, one leg, one arm.
    fn rig() -> (Rig, Vec<i32>) {
        let tags = vec![0, PELVIS, SPINE0, 0x60F0, 0x60F1, 0x60F2, 0x9995, 0x796E, L_THIGH, 0xF9BB, 0x3779, 0x083C, R_THIGH, 0xFCD9, 0xB1C5, 0xEEEB, 0x49D9, 0x1234];
        let parents = vec![-1, 0, 1, 2, 3, 4, 5, 6, 1, 8, 9, 10, 1, 5, 13, 14, 15, 16];
        let p = [
            v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.1), v(0.0, 0.0, 0.2), v(0.0, 0.0, 0.3),
            v(0.0, 0.0, 0.4), v(0.0, 0.0, 0.55), v(0.0, 0.0, 0.65), v(-0.1, 0.0, -0.05), v(-0.1, 0.0, -0.5),
            v(-0.1, 0.0, -0.9), v(-0.1, 0.12, -0.95), v(0.1, 0.0, -0.05), v(-0.05, 0.0, 0.45), v(-0.2, 0.0, 0.45),
            v(-0.2, 0.0, 0.15), v(-0.2, 0.0, -0.1), v(-0.2, 0.02, -0.15),
        ];
        let identity = [v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0), v(0.0, 0.0, 1.0)];
        let rest = p.iter().map(|&position| BonePose { axes: identity, position }).collect();
        let index_of = {
            let tags = tags.clone();
            move |tag: i32| tags.iter().position(|&t| t == tag)
        };
        (Rig::new(rest, parents.clone(), index_of).unwrap(), tags)
    }

    fn skate_joints(arm_forward: bool) -> impl Fn(&str) -> Option<GtaVec> {
        move |name: &str| {
            Some(match name {
                "HIPS" => v(0.0, 0.0, 0.0),
                "SPINE" => v(0.0, 0.0, 0.1),
                "SPINE1" => v(0.0, 0.0, 0.2),
                "SPINE2" => v(0.0, 0.0, 0.3),
                "SPINE3" => v(0.0, 0.0, 0.4),
                "NECK" => v(0.0, 0.0, 0.55),
                "HEAD" => v(0.0, 0.0, 0.65),
                "LEFTUPLEG" => v(-0.1, 0.0, -0.05),
                "RIGHTUPLEG" => v(0.1, 0.0, -0.05),
                "LEFTLEG" => v(-0.1, 0.0, -0.5),
                "LEFTFOOT" => v(-0.1, 0.0, -0.9),
                "LEFTTOEBASE" => v(-0.1, 0.12, -0.95),
                "LEFTSHOULDER" => v(-0.05, 0.0, 0.45),
                "LEFTARM" => v(-0.2, 0.0, 0.45),
                "LEFTFOREARM" => if arm_forward { v(-0.2, 0.3, 0.45) } else { v(-0.2, 0.0, 0.15) },
                "LEFTHAND" => if arm_forward { v(-0.2, 0.55, 0.45) } else { v(-0.2, 0.0, -0.1) },
                _ => return None,
            })
        }
    }

    fn close(a: GtaVec, b: GtaVec) -> bool {
        length(a.sub(b)) < 1e-3
    }

    #[test]
    fn matching_skate_pose_reproduces_the_rest_pose() {
        let (rig, _) = rig();
        let pose = rig.solve(skate_joints(false));
        for (i, (out, rest)) in pose.iter().zip(&rig.rest).enumerate() {
            assert!(close(out.position, rest.position), "bone {i}");
            for k in 0..3 {
                assert!(close(out.axes[k], rest.axes[k]), "bone {i} axis {k}");
            }
        }
    }

    #[test]
    fn raised_arm_points_forward_and_children_follow() {
        let (rig, tags) = rig();
        let pose = rig.solve(skate_joints(true));
        let at = |tag: i32| pose[tags.iter().position(|&t| t == tag).unwrap()];
        let upper = at(0xB1C5);
        let fore = at(0xEEEB);
        let hand = at(0x49D9);
        assert!(close(fore.position.sub(upper.position).normalized(), v(0.0, 1.0, 0.0)));
        assert!((length(fore.position.sub(upper.position)) - 0.3).abs() < 1e-3, "keeps the GTA bone length");
        assert!(close(hand.position.sub(fore.position).normalized(), v(0.0, 1.0, 0.0)));
        let finger = pose[17];
        assert!((length(finger.position.sub(hand.position)) - (0.02f32 * 0.02 + 0.05 * 0.05).sqrt()).abs() < 1e-3, "unmapped bones ride rigidly");
    }

    #[test]
    fn hips_turning_turns_the_whole_body() {
        let (rig, _) = rig();
        let turned = |name: &str| {
            skate_joints(false)(name).map(|p| v(-p.y, p.x, p.z)) // 90 degrees about up
        };
        let pose = rig.solve(turned);
        let head = pose[7].position;
        assert!(close(head, v(0.0, 0.0, 0.65)));
        let foot = pose[10].position;
        assert!(close(foot, v(0.0, -0.1, -0.9)), "{foot:?}");
        assert!(close(pose[1].axes[0], v(0.0, 1.0, 0.0)), "pelvis X axis turned to +Y");
    }
}
