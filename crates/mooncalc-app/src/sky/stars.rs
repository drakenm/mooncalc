//! The embedded bright-star list and its reduction to the equinox of date.
//!
//! The data are every star brighter than V = 3.5 in the Yale Bright Star
//! Catalogue, 5th revised edition (Hoffleit & Warren 1991, public domain),
//! converted by `tools/stars/build_stars.py`; see `tools/stars/README.md`.

use mooncalc_core::angle::DEG;
use mooncalc_core::matrix::{apply, from_spherical, to_spherical};
use mooncalc_core::precession::j2000_to_true_of_date_matrix;

pub use super::stars_data::BRIGHT_STARS;

/// One catalogue star (J2000.0 / FK5 position).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Star {
    /// Harvard Revised (HR) number.
    pub hr: u16,
    pub ra_deg: f64,
    pub dec_deg: f64,
    /// Visual magnitude V.
    pub vmag: f64,
    /// B−V colour index.
    pub bv: f64,
    /// Bayer or Flamsteed designation, e.g. "α CMa"; may be empty.
    pub label: &'static str,
    /// IAU proper name for the brightest stars; may be empty.
    pub name: &'static str,
}

impl Star {
    /// The best short label: the proper name, else the designation.
    pub fn display_name(&self) -> &'static str {
        if self.name.is_empty() {
            self.label
        } else {
            self.name
        }
    }
}

/// A star's position referred to the true equator and equinox of date.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StarOfDate {
    pub star: &'static Star,
    /// Right ascension, radians.
    pub ra: f64,
    /// Declination, radians.
    pub dec: f64,
}

/// Every catalogue star precessed and nutated from J2000.0 to the true
/// equator and equinox of TT Julian Day `jd_tt` (IAU 1976 precession, IAU
/// 1980 nutation: [`j2000_to_true_of_date_matrix`]). Proper motion and
/// annual aberration (≤ 20″) are neglected; both are invisible at chart
/// scale.
pub fn stars_of_date(jd_tt: f64) -> Vec<StarOfDate> {
    let m = j2000_to_true_of_date_matrix(jd_tt);
    BRIGHT_STARS
        .iter()
        .map(|star| {
            let v = apply(
                &m,
                &from_spherical(star.ra_deg * DEG, star.dec_deg * DEG, 1.0),
            );
            let (ra, dec, _) = to_spherical(&v);
            StarOfDate { star, ra, dec }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mooncalc_core::angle::ARCSEC;
    use mooncalc_core::precession::precession_matrix;
    use mooncalc_core::time::J2000;

    #[test]
    fn catalogue_is_sane() {
        assert!((250..=320).contains(&BRIGHT_STARS.len()));
        assert!(BRIGHT_STARS.iter().all(|s| s.vmag < 3.5));
        assert!(BRIGHT_STARS.windows(2).all(|w| w[0].vmag <= w[1].vmag));
        let sirius = &BRIGHT_STARS[0];
        assert_eq!(sirius.name, "Sirius");
        assert_eq!(sirius.display_name(), "Sirius");
        // BSC5 J2000: 06h45m08.9s −16°42′58″.
        assert!((sirius.ra_deg - (6.0 + 45.0 / 60.0 + 8.9 / 3600.0) * 15.0).abs() < 1e-3);
        assert!((sirius.dec_deg + (16.0 + 42.0 / 60.0 + 58.0 / 3600.0)).abs() < 1e-3);
        assert!(BRIGHT_STARS.iter().any(|s| s.name == "Polaris"));
    }

    #[test]
    fn reduction_to_date() {
        // At J2000 the reduction is just nutation (< 20″).
        let at_j2000 = stars_of_date(J2000);
        let s = &at_j2000[0];
        assert!((s.ra / DEG - s.star.ra_deg).abs() * 3600.0 < 25.0);
        // Meeus, Astronomical Algorithms, example 21.b: θ Persei, J2000
        // α = 2h44m11.986s, δ = +49°13′42.48″, μα = +0.03425 s/yr,
        // μδ = −0.0895″/yr, to JDE 2462088.69 (2028 Nov 13.19 TD): mean
        // place α = 2h46m11.331s, δ = +49°20′54.54″. Apply the proper
        // motion, then the precession used by `stars_of_date`.
        let jd = 2_462_088.69;
        let years = (jd - J2000) / 365.25;
        let ra0 =
            ((2.0 + 44.0 / 60.0 + 11.986 / 3600.0) * 15.0 + years * 0.03425 * 15.0 / 3600.0) * DEG;
        let dec0 = (49.0 + 13.0 / 60.0 + 42.48 / 3600.0 - years * 0.0895 / 3600.0) * DEG;
        let p = precession_matrix(J2000, jd);
        let (ra, dec, _) = to_spherical(&apply(&p, &from_spherical(ra0, dec0, 1.0)));
        let ra_ex = (2.0 + 46.0 / 60.0 + 11.331 / 3600.0) * 15.0 * DEG;
        let dec_ex = (49.0 + 20.0 / 60.0 + 54.54 / 3600.0) * DEG;
        assert!(
            (ra - ra_ex).abs() < 0.2 * ARCSEC,
            "{}",
            (ra - ra_ex) / ARCSEC
        );
        assert!(
            (dec - dec_ex).abs() < 0.2 * ARCSEC,
            "{}",
            (dec - dec_ex) / ARCSEC
        );
        // The true place adds nutation only (≲ 20″).
        let t = apply(
            &j2000_to_true_of_date_matrix(jd),
            &from_spherical(ra0, dec0, 1.0),
        );
        let (rt, dt, _) = to_spherical(&t);
        assert!((rt - ra).abs() < 40.0 * ARCSEC && (dt - dec).abs() < 20.0 * ARCSEC);
    }
}
