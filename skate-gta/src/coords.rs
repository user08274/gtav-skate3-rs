//! GTA V world space (metres, Z up, Y north) <-> Skate 3 physics space
//! (metres, Y up). The mapping is a proper rotation about X:
//! skate = (gx, gz, -gy), gta = (sx, -sz, sy). Skate positions are kept
//! relative to a floating origin so the solver works with small numbers.
use skate_core::{
    math::{Basis3, Vector3},
    physics::drive_frames::RetailAffineTransform,
};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GtaVec {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl GtaVec {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
    pub fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
    pub fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
    pub fn scale(self, s: f32) -> Self {
        Self::new(self.x * s, self.y * s, self.z * s)
    }
}

pub fn dir_to_skate(v: GtaVec) -> Vector3 {
    Vector3::new(v.x, v.z, -v.y)
}

pub fn dir_to_gta(v: Vector3) -> GtaVec {
    GtaVec::new(v.x, -v.z, v.y)
}

/// Skate space anchored at `origin` and turned `yaw` radians about GTA up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub origin: GtaVec,
    pub yaw: f32,
}

impl Frame {
    pub fn new(origin: GtaVec) -> Self {
        Self { origin, yaw: 0.0 }
    }
    /// Orient skate +Z (the stock spawn's board forward) along a GTA heading.
    pub fn facing(origin: GtaVec, heading_degrees: f32) -> Self {
        Self { origin, yaw: heading_degrees.to_radians() + std::f32::consts::PI }
    }
    fn rotate(&self, v: GtaVec, sign: f32) -> GtaVec {
        let (s, c) = (self.yaw * sign).sin_cos();
        GtaVec::new(c * v.x - s * v.y, s * v.x + c * v.y, v.z)
    }
    pub fn dir_to_skate(&self, v: GtaVec) -> Vector3 {
        dir_to_skate(self.rotate(v, -1.0))
    }
    pub fn dir_to_gta(&self, v: Vector3) -> GtaVec {
        self.rotate(dir_to_gta(v), 1.0)
    }
    pub fn to_skate(&self, p: GtaVec) -> Vector3 {
        self.dir_to_skate(p.sub(self.origin))
    }
    pub fn to_gta(&self, p: Vector3) -> GtaVec {
        self.dir_to_gta(p).add(self.origin)
    }
    /// GTA entity axes of a skate basis (`Ri, Up, At` columns).
    pub fn entity_axes(&self, basis: Basis3) -> EntityAxes {
        let col = |i: usize| {
            let c = basis.columns[i];
            self.dir_to_gta(Vector3::new(c[0], c[1], c[2]))
        };
        EntityAxes { right: col(0).scale(-1.0), forward: col(2), up: col(1) }
    }
}

/// Rotation about skate +Y that points the board's `At` (+Z) axis along a
/// GTA heading (degrees, 0 = north, counter-clockwise).
pub fn basis_for_heading(heading_degrees: f32) -> Basis3 {
    let a = heading_degrees.to_radians() + std::f32::consts::PI;
    let (s, c) = a.sin_cos();
    Basis3 {
        columns: [[c, 0.0, -s], [0.0, 1.0, 0.0], [s, 0.0, c]],
    }
}

/// GTA entity axes for a skate part pose. Skate columns are `Ri, Up, At`;
/// GTA entities use right (X), forward (Y), up (Z) with right x forward = up.
pub struct EntityAxes {
    pub right: GtaVec,
    pub forward: GtaVec,
    pub up: GtaVec,
}

pub fn entity_axes(basis: Basis3) -> EntityAxes {
    let col = |i: usize| {
        let c = basis.columns[i];
        dir_to_gta(Vector3::new(c[0], c[1], c[2]))
    };
    EntityAxes {
        right: col(0).scale(-1.0),
        forward: col(2),
        up: col(1),
    }
}

/// Quaternion (x, y, z, w) of the rotation whose columns are the entity axes.
pub fn quaternion(axes: &EntityAxes) -> [f32; 4] {
    let (m00, m10, m20) = (axes.right.x, axes.right.y, axes.right.z);
    let (m01, m11, m21) = (axes.forward.x, axes.forward.y, axes.forward.z);
    let (m02, m12, m22) = (axes.up.x, axes.up.y, axes.up.z);
    let trace = m00 + m11 + m22;
    let q = if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        [(m21 - m12) / s, (m02 - m20) / s, (m10 - m01) / s, 0.25 * s]
    } else if m00 > m11 && m00 > m22 {
        let s = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
        [0.25 * s, (m01 + m10) / s, (m02 + m20) / s, (m21 - m12) / s]
    } else if m11 > m22 {
        let s = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
        [(m01 + m10) / s, 0.25 * s, (m12 + m21) / s, (m02 - m20) / s]
    } else {
        let s = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
        [(m02 + m20) / s, (m12 + m21) / s, 0.25 * s, (m10 - m01) / s]
    };
    let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    q.map(|v| v / n)
}

/// Quaternion product a * b, both (x, y, z, w).
pub fn quaternion_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

/// Rotation about the entity's local Z (up) axis, for model yaw offsets.
pub fn yaw_quaternion(degrees: f32) -> [f32; 4] {
    let (s, c) = (degrees.to_radians() * 0.5).sin_cos();
    [0.0, 0.0, s, c]
}

/// GTA heading (degrees, 0 = north, counter-clockwise) of a horizontal direction.
pub fn heading_degrees(forward: GtaVec) -> f32 {
    (-forward.x).atan2(forward.y).to_degrees().rem_euclid(360.0)
}

pub fn spawn_transform(position: Vector3, heading_degrees: f32) -> RetailAffineTransform {
    RetailAffineTransform {
        basis: basis_for_heading(heading_degrees),
        translation: position,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: GtaVec, b: GtaVec) -> bool {
        (a.x - b.x).abs() < 1e-5 && (a.y - b.y).abs() < 1e-5 && (a.z - b.z).abs() < 1e-5
    }

    #[test]
    fn positions_round_trip_through_the_floating_origin() {
        let frame = Frame::new(GtaVec::new(-1200.0, 300.0, 40.0));
        let p = GtaVec::new(-1195.5, 310.25, 41.0);
        let s = frame.to_skate(p);
        assert_eq!(s.y, 1.0);
        assert!(close(frame.to_gta(s), p));
    }

    #[test]
    fn a_facing_frame_points_skate_forward_along_the_heading() {
        for heading in [0.0_f32, 90.0, 200.0] {
            let frame = Frame::facing(GtaVec::new(5.0, 6.0, 7.0), heading);
            let h = heading.to_radians();
            let forward = frame.dir_to_gta(Vector3::new(0.0, 0.0, 1.0));
            assert!(close(forward, GtaVec::new(-h.sin(), h.cos(), 0.0)), "{heading}");
            let p = GtaVec::new(9.0, -3.0, 8.5);
            assert!(close(frame.to_gta(frame.to_skate(p)), p));
            let axes = frame.entity_axes(basis_for_heading(0.0));
            assert!(close(axes.up, GtaVec::new(0.0, 0.0, 1.0)));
        }
    }

    #[test]
    fn gta_up_is_skate_up() {
        let up = dir_to_skate(GtaVec::new(0.0, 0.0, 1.0));
        assert_eq!((up.x, up.y, up.z), (0.0, 1.0, 0.0));
    }

    #[test]
    fn heading_basis_points_board_forward_along_the_ped_heading() {
        for heading in [0.0_f32, 37.0, 90.0, 180.0, 271.0] {
            let axes = entity_axes(basis_for_heading(heading));
            let h = heading.to_radians();
            assert!(close(axes.forward, GtaVec::new(-h.sin(), h.cos(), 0.0)), "{heading}");
            assert!(close(axes.up, GtaVec::new(0.0, 0.0, 1.0)));
            let r = axes.right;
            let f = axes.forward;
            let cross = GtaVec::new(r.y * f.z - r.z * f.y, r.z * f.x - r.x * f.z, r.x * f.y - r.y * f.x);
            assert!(close(cross, axes.up), "right x forward must be up");
        }
    }

    #[test]
    fn heading_round_trips_through_the_board_basis() {
        for heading in [0.0_f32, 45.0, 135.0, 270.0, 359.0] {
            let forward = entity_axes(basis_for_heading(heading)).forward;
            assert!((heading_degrees(forward) - heading).abs() < 1e-3, "{heading}");
        }
    }

    #[test]
    fn identity_entity_axes_give_identity_quaternion() {
        let axes = EntityAxes {
            right: GtaVec::new(1.0, 0.0, 0.0),
            forward: GtaVec::new(0.0, 1.0, 0.0),
            up: GtaVec::new(0.0, 0.0, 1.0),
        };
        let q = quaternion(&axes);
        assert!((q[3] - 1.0).abs() < 1e-6 && q[0].abs() < 1e-6);
    }

    #[test]
    fn heading_quaternion_matches_yaw() {
        let q = quaternion(&entity_axes(basis_for_heading(90.0)));
        let y = yaw_quaternion(90.0);
        let dot: f32 = q.iter().zip(y).map(|(a, b)| a * b).sum();
        assert!(dot.abs() > 0.9999);
    }
}
