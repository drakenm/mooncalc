//! Observing circumstances of the young (or old) crescent.

/// Everything a sighting criterion may need, for one observer at one moment
/// (normally the criterion's "best time"; see each criterion's docs).
///
/// Field naming follows the literature (Yallop 1997, NAO TN 69; Odeh 2004).
/// Unit suffixes are explicit; angles here are **degrees** for readability
/// against published tables.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct CrescentGeometry {
    /// ARCL: Sun–Moon elongation (topocentric unless a criterion says
    /// otherwise), degrees.
    pub arcl_deg: f64,
    /// Geocentric elongation, degrees.
    pub arcl_geo_deg: f64,
    /// ARCV: airless altitude difference Moon − Sun (geocentric Moon
    /// altitude, as in Yallop/Odeh), degrees.
    pub arcv_deg: f64,
    /// DAZ: azimuth difference Sun − Moon, degrees.
    pub daz_deg: f64,
    /// Topocentric crescent width W′, arc minutes.
    pub width_topo_arcmin: f64,
    /// Geocentric crescent width W, arc minutes.
    pub width_geo_arcmin: f64,
    /// Topocentric lunar semi-diameter, arc minutes.
    pub semi_diameter_topo_arcmin: f64,
    /// Moon's apparent (refracted) altitude, degrees.
    pub moon_alt_deg: f64,
    /// Moon's airless topocentric altitude, degrees.
    pub moon_alt_airless_deg: f64,
    /// Sun's apparent altitude, degrees (negative below horizon).
    pub sun_alt_deg: f64,
    /// Moon's azimuth, degrees.
    pub moon_az_deg: f64,
    /// Sun's azimuth, degrees.
    pub sun_az_deg: f64,
    /// Lag: moonset − sunset (evening) or sunrise − moonrise (morning),
    /// minutes. Negative when the Moon sets before the Sun.
    pub lag_minutes: f64,
    /// Moon age since conjunction (geocentric), hours. Negative before
    /// conjunction.
    pub age_hours: f64,
    /// Illuminated fraction of the lunar disc, 0..1.
    pub illuminated_fraction: f64,
    /// Moon's distance from the geocentre, km.
    pub moon_distance_km: f64,
    /// Topocentric airless ARCV, degrees: the Moon's **topocentric**
    /// altitude minus the Sun's altitude, both centre-to-centre and **without
    /// refraction**, at the evaluation time. The Sun's parallax (< 9″) may be
    /// neglected. Unlike [`Self::arcv_deg`] (geocentric Moon, Yallop's
    /// definition), this is the quantity Odeh (2004, Exp. Astron. 18, 39, §5)
    /// fits: "ARCV1, ARCV2 and ARCV3 are airless and topocentric".
    pub arcv_topo_deg: f64,
}
