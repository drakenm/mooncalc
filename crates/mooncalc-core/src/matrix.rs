//! 3×3 rotation matrices and 3-vectors.
//!
//! Rotations follow the usual astrometric (frame-rotation) convention, as in
//! the IERS Conventions and the SOFA library: `rot_x(φ)`, `rot_y(φ)`,
//! `rot_z(φ)` rotate the *coordinate frame* by `+φ` about the axis, so a
//! fixed vector appears rotated by `−φ`. A matrix maps column vectors:
//! `v' = M v`.

/// Row-major 3×3 matrix.
pub type Mat3 = [[f64; 3]; 3];
/// Cartesian 3-vector.
pub type Vec3 = [f64; 3];

/// The identity matrix.
pub const IDENTITY: Mat3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// Frame rotation about the x axis.
pub fn rot_x(phi: f64) -> Mat3 {
    let (s, c) = phi.sin_cos();
    [[1.0, 0.0, 0.0], [0.0, c, s], [0.0, -s, c]]
}

/// Frame rotation about the y axis.
pub fn rot_y(phi: f64) -> Mat3 {
    let (s, c) = phi.sin_cos();
    [[c, 0.0, -s], [0.0, 1.0, 0.0], [s, 0.0, c]]
}

/// Frame rotation about the z axis.
pub fn rot_z(phi: f64) -> Mat3 {
    let (s, c) = phi.sin_cos();
    [[c, s, 0.0], [-s, c, 0.0], [0.0, 0.0, 1.0]]
}

/// Matrix product `a · b` (apply `b` first, then `a`).
pub fn mul(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut r = [[0.0; 3]; 3];
    for (i, row) in r.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    r
}

/// Transpose (the inverse, for a rotation matrix).
pub fn transpose(a: &Mat3) -> Mat3 {
    [
        [a[0][0], a[1][0], a[2][0]],
        [a[0][1], a[1][1], a[2][1]],
        [a[0][2], a[1][2], a[2][2]],
    ]
}

/// Matrix × vector.
#[inline]
pub fn apply(m: &Mat3, v: &Vec3) -> Vec3 {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

/// Spherical (longitude-like, latitude-like, radius) to Cartesian.
#[inline]
pub fn from_spherical(lon: f64, lat: f64, r: f64) -> Vec3 {
    let (sl, cl) = lon.sin_cos();
    let (sb, cb) = lat.sin_cos();
    [r * cb * cl, r * cb * sl, r * sb]
}

/// Cartesian to (longitude in `[0, 2π)`, latitude, radius).
#[inline]
pub fn to_spherical(v: &Vec3) -> (f64, f64, f64) {
    let rxy2 = v[0] * v[0] + v[1] * v[1];
    let r = (rxy2 + v[2] * v[2]).sqrt();
    let lon = crate::angle::norm_2pi(v[1].atan2(v[0]));
    let lat = v[2].atan2(rxy2.sqrt());
    (lon, lat, r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_sense() {
        // Rotating the frame by +90° about z makes the x axis appear at −90°.
        let v = apply(&rot_z(std::f64::consts::FRAC_PI_2), &[1.0, 0.0, 0.0]);
        assert!((v[1] + 1.0).abs() < 1e-15 && v[0].abs() < 1e-15);
        let m = mul(&rot_x(0.3), &rot_y(-0.7));
        let i = mul(&m, &transpose(&m));
        for (r, row) in i.iter().enumerate() {
            for (c, x) in row.iter().enumerate() {
                assert!((x - IDENTITY[r][c]).abs() < 1e-15);
            }
        }
    }

    #[test]
    fn spherical_round_trip() {
        let (lon, lat, r) = to_spherical(&from_spherical(4.0, -0.5, 2.0));
        assert!((lon - 4.0).abs() < 1e-14 && (lat + 0.5).abs() < 1e-14 && (r - 2.0).abs() < 1e-14);
    }
}
