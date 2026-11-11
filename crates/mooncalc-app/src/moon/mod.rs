//! The Moon close-up: physical ephemeris (libration, axis position angle,
//! the Sun's selenographic position), phase and orientation, the monthly
//! libration curve, and the near-side feature layer.
//!
//! | Operation | Input | Output |
//! | --- | --- | --- |
//! | [`moon_view`] | [`MoonViewParams`] | [`MoonView`] |
//! | [`moon_features`] | none | [`MoonFeatures`] |
//!
//! Sources: Meeus, *Astronomical Algorithms* 2nd ed., ch. 48 (illuminated
//! fraction, bright limb χ), ch. 53 (librations, P, selenographic Sun,
//! lunar sunrise), eq. 14.1 (parallactic angle), via
//! [`mooncalc_core::tables::ephemeris_row`] and
//! [`mooncalc_core::libration`]. The submodules are shared with the egui
//! Moon tab: [`features`] (IAU Gazetteer craters, schematic maria),
//! [`disc`] (the disc projection and terminator) and [`libration`] (the
//! lunation's libration curve).

pub mod disc;
pub mod features;
pub mod libration;

use mooncalc_core::angle::{DEG, norm_2pi};
use mooncalc_core::events::Phase;
use mooncalc_core::events::phases::{SYNODIC_MONTH_DAYS, next_phase};
use mooncalc_core::libration::{MoonPhysical, lunar_sunrise, lunar_sunset};
use mooncalc_core::tables::{TableOptions, ephemeris_row};
use mooncalc_core::{Ephemeris, GeoLocation, Instant};
use serde::{Deserialize, Serialize};

use crate::dto::{ApiError, Location, UtDateTime};
use crate::ephemeris::check_covers;
use crate::sky::geometry::{parallactic_angle, phase_name};
use crate::validate;
use features::{CRATERS, MARIA, MareShape};
use libration::LunationCurve;

/// The next instant after `jd_tt` (TT) at which the Sun rises (`rise`) or
/// sets at selenographic longitude `eta`, latitude `theta` (radians), by
/// the iteration of Meeus ch. 53 ([`lunar_sunrise`], [`lunar_sunset`]),
/// stepping a lunation at a time until the event follows `jd_tt`.
pub fn next_lunar_sun_event(
    eph: &dyn Ephemeris,
    eta: f64,
    theta: f64,
    jd_tt: f64,
    rise: bool,
) -> Option<f64> {
    let f = |jd: f64| {
        if rise {
            lunar_sunrise(eph, eta, theta, jd)
        } else {
            lunar_sunset(eph, eta, theta, jd)
        }
    };
    let mut t = f(jd_tt);
    for _ in 0..3 {
        if !t.is_finite() {
            return None;
        }
        if t > jd_tt {
            return Some(t);
        }
        t = f(t + SYNODIC_MONTH_DAYS);
    }
    None
}

fn yes() -> bool {
    true
}

/// Input of `moon_view`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoonViewParams {
    pub instant: UtDateTime,
    /// Observer: adds the topocentric librations, illumination, the
    /// parallactic angle and altitude/azimuth. None gives geocentric only.
    #[serde(default)]
    pub location: Option<Location>,
    #[serde(default)]
    pub ephemeris_id: Option<String>,
    /// A crater name from [`moon_features`]: adds the Sun's altitude there
    /// and its next sunrise and sunset.
    #[serde(default)]
    pub crater: Option<String>,
    /// Send the lunation's libration curve (default true).
    #[serde(default = "yes")]
    pub include_track: bool,
}

/// Optical, physical and total librations (Meeus ch. 53).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrationDto {
    pub optical_lon_deg: f64,
    pub optical_lat_deg: f64,
    pub physical_lon_deg: f64,
    pub physical_lat_deg: f64,
    /// Total libration in longitude l (sub-observer selenographic longitude).
    pub lon_deg: f64,
    /// Total libration in latitude b.
    pub lat_deg: f64,
}

/// The Moon's physical ephemeris for one viewpoint.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoonPhysicalDto {
    pub libration: LibrationDto,
    /// Position angle P of the axis (north pole), from celestial north
    /// through east.
    pub axis_position_angle_deg: f64,
    /// Selenographic longitude l₀ and latitude b₀ of the subsolar point.
    pub sun_selenographic_lon_deg: f64,
    pub sun_selenographic_lat_deg: f64,
    /// Selenographic colongitude of the Sun, c₀ = 90° − l₀.
    pub colongitude_deg: f64,
}

impl From<&MoonPhysical> for MoonPhysicalDto {
    fn from(p: &MoonPhysical) -> Self {
        let l = &p.libration;
        Self {
            libration: LibrationDto {
                optical_lon_deg: l.optical_lon / DEG,
                optical_lat_deg: l.optical_lat / DEG,
                physical_lon_deg: l.physical_lon / DEG,
                physical_lat_deg: l.physical_lat / DEG,
                lon_deg: l.lon / DEG,
                lat_deg: l.lat / DEG,
            },
            axis_position_angle_deg: l.position_angle / DEG,
            sun_selenographic_lon_deg: p.sun_lon / DEG,
            sun_selenographic_lat_deg: p.sun_lat / DEG,
            colongitude_deg: p.colongitude / DEG,
        }
    }
}

/// What a frontend needs to draw the disc ([`disc::View`]): the
/// sub-observer point, the subsolar point and the screen rotation of the
/// lunar north pole, anticlockwise from up, for a north-up or zenith-up
/// display (sky as seen: east left of north).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscView {
    pub l_deg: f64,
    pub b_deg: f64,
    pub sun_lon_deg: f64,
    pub sun_lat_deg: f64,
    /// P.
    pub north_up_rotation_deg: f64,
    /// P − q, when a location is given.
    pub zenith_up_rotation_deg: Option<f64>,
}

/// The Sun at a chosen crater.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CraterSun {
    pub name: String,
    /// Altitude of the Sun above the crater's horizon (Meeus ch. 53).
    pub sun_alt_deg: f64,
    pub next_sunrise: Option<UtDateTime>,
    pub next_sunset: Option<UtDateTime>,
}

/// Libration in longitude and latitude through the lunation containing
/// the instant (geocentric), every six hours.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrationTrack {
    pub new_moon: UtDateTime,
    pub full_moon: UtDateTime,
    pub next_new_moon: UtDateTime,
    /// `[l, b]` in degrees, evenly spaced from `new_moon` to `next_new_moon`.
    pub points: Vec<[f64; 2]>,
    /// The geocentric `[l, b]` at the instant.
    pub now: [f64; 2],
    /// The geocentric `[l, b]` at the full Moon.
    pub full: [f64; 2],
}

/// The Moon at one instant.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoonView {
    pub instant: UtDateTime,
    pub ephemeris: String,
    /// Whether the headline values (illumination, χ, librations in
    /// `disc`, diameter, distance) are topocentric.
    pub topocentric: bool,
    pub geocentric: MoonPhysicalDto,
    pub topocentric_physical: Option<MoonPhysicalDto>,
    pub disc: DiscView,
    pub illuminated_fraction: f64,
    pub phase_angle_deg: f64,
    pub elongation_deg: f64,
    /// Position angle χ of the bright limb from north (Meeus eq. 48.5).
    pub bright_limb_deg: f64,
    /// Parallactic angle q, when a location is given.
    pub parallactic_deg: Option<f64>,
    /// χ − q, when a location is given.
    pub bright_limb_zenith_deg: Option<f64>,
    /// Geocentric λ☾ − λ☉, `[0, 360)`.
    pub elongation_lon_deg: f64,
    pub phase_name: String,
    /// Days since the last geocentric conjunction.
    pub age_days: f64,
    pub last_new_moon: UtDateTime,
    pub next_new_moon: UtDateTime,
    pub next_first_quarter: UtDateTime,
    pub next_full_moon: UtDateTime,
    pub next_last_quarter: UtDateTime,
    /// Observer-to-Moon distance (geocentric without a location), km.
    pub distance_km: f64,
    pub geocentric_distance_km: f64,
    pub apparent_diameter_arcmin: f64,
    /// Apparent right ascension and declination (topocentric with a location).
    pub ra_deg: f64,
    pub dec_deg: f64,
    /// Apparent (refracted) altitude and azimuth, when a location is given.
    pub alt_deg: Option<f64>,
    pub az_deg: Option<f64>,
    pub crater: Option<CraterSun>,
    pub libration_track: Option<LibrationTrack>,
}

/// A named crater.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CraterDto {
    pub name: String,
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub diameter_km: f64,
}

/// A mare: its label point and its closed outline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MareDto {
    pub name: String,
    pub lat_deg: f64,
    pub lon_deg: f64,
    /// `true` when the outline is the Gazetteer-diameter circle, `false`
    /// for a hand-placed polygon.
    pub circular: bool,
    /// Closed outline, `[lat0, lon0, lat1, lon1, ...]` in degrees.
    pub outline: Vec<f32>,
}

/// The near-side feature layer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoonFeatures {
    pub craters: Vec<CraterDto>,
    pub maria: Vec<MareDto>,
    /// Mean lunar radius, km.
    pub moon_radius_km: f64,
    pub attribution: String,
}

fn ut(t: Instant) -> UtDateTime {
    t.into()
}

/// The Moon's phase, orientation, librations and distances at an instant.
pub fn moon_view(eph: &dyn Ephemeris, p: &MoonViewParams) -> Result<MoonView, ApiError> {
    validate::date(p.instant.date())?;
    if let Some(l) = p.location {
        validate::location(l)?;
    }
    let instant = p.instant.to_instant();
    let jd_tt = instant.jd_tt();
    check_covers(eph, jd_tt - 32.0, jd_tt + 32.0)?;
    let crater = match &p.crater {
        Some(name) => Some(
            CRATERS
                .iter()
                .find(|c| c.name.eq_ignore_ascii_case(name.trim()))
                .ok_or_else(|| ApiError::invalid_params(format!("unknown crater {name:?}")))?,
        ),
        None => None,
    };
    let loc = p.location.map(GeoLocation::from);
    let row = ephemeris_row(
        eph,
        instant,
        &TableOptions {
            location: loc,
            ..TableOptions::default()
        },
    );
    let topo = row.moon.topocentric;
    let physical = row.topocentric_physical.unwrap_or(row.physical);
    let illum = row.topocentric_illumination.unwrap_or(row.illumination);
    let q = match (loc, topo) {
        (Some(l), Some(t)) => {
            let lst = instant.local_apparent_sidereal_time(l.lon_deg * DEG);
            Some(parallactic_angle(
                lst - t.equatorial.ra,
                t.equatorial.dec,
                l.lat_deg * DEG,
            ))
        }
        _ => None,
    };
    let pa = physical.libration.position_angle;
    let elong_lon = norm_2pi(row.moon.ecliptic.lon - row.sun.ecliptic.lon) / DEG;
    let (eq, semi) = topo.map_or((row.moon.equatorial, row.moon.semidiameter), |t| {
        (t.equatorial, t.semidiameter)
    });
    let crater = crater.map(|c| {
        let (eta, theta) = (c.lon * DEG, c.lat * DEG);
        CraterSun {
            name: c.name.into(),
            sun_alt_deg: row.physical.sun_altitude_at(eta, theta) / DEG,
            next_sunrise: next_lunar_sun_event(eph, eta, theta, jd_tt, true)
                .map(|t| ut(Instant::from_jd_tt(t))),
            next_sunset: next_lunar_sun_event(eph, eta, theta, jd_tt, false)
                .map(|t| ut(Instant::from_jd_tt(t))),
        }
    });
    let libration_track = p.include_track.then(|| {
        let c = LunationCurve::around(eph, instant);
        LibrationTrack {
            new_moon: ut(c.new_moon),
            full_moon: ut(c.full_moon),
            next_new_moon: ut(c.next_new_moon),
            now: libration::lb_deg(eph, instant),
            full: libration::lb_deg(eph, c.full_moon),
            points: c.points,
        }
    });
    let next = |ph| ut(next_phase(eph, ph, instant));
    Ok(MoonView {
        instant: p.instant,
        ephemeris: eph.name().to_owned(),
        topocentric: loc.is_some(),
        geocentric: (&row.physical).into(),
        topocentric_physical: row.topocentric_physical.as_ref().map(Into::into),
        disc: DiscView {
            l_deg: physical.libration.lon / DEG,
            b_deg: physical.libration.lat / DEG,
            sun_lon_deg: physical.sun_lon / DEG,
            sun_lat_deg: physical.sun_lat / DEG,
            north_up_rotation_deg: pa / DEG,
            zenith_up_rotation_deg: q.map(|q| norm_2pi(pa - q) / DEG),
        },
        illuminated_fraction: illum.fraction,
        phase_angle_deg: illum.phase_angle / DEG,
        elongation_deg: illum.elongation / DEG,
        bright_limb_deg: illum.bright_limb_angle / DEG,
        parallactic_deg: q.map(|q| q / DEG),
        bright_limb_zenith_deg: q.map(|q| norm_2pi(illum.bright_limb_angle - q) / DEG),
        elongation_lon_deg: elong_lon,
        phase_name: phase_name(elong_lon).into(),
        age_days: row.moon_age_days,
        last_new_moon: ut(Instant::from_jd_tt(row.conjunction_jd_tt)),
        next_new_moon: next(Phase::New),
        next_first_quarter: next(Phase::FirstQuarter),
        next_full_moon: next(Phase::Full),
        next_last_quarter: next(Phase::LastQuarter),
        distance_km: eq.distance_km,
        geocentric_distance_km: row.moon.equatorial.distance_km,
        apparent_diameter_arcmin: 2.0 * semi / DEG * 60.0,
        ra_deg: norm_2pi(eq.ra) / DEG,
        dec_deg: eq.dec / DEG,
        alt_deg: topo.map(|t| t.apparent_altitude / DEG),
        az_deg: topo.map(|t| norm_2pi(t.horizontal.az) / DEG),
        crater,
        libration_track,
    })
}

/// The near-side craters and maria of [`features`], with mare outlines
/// densified for drawing.
pub fn moon_features() -> MoonFeatures {
    MoonFeatures {
        craters: CRATERS
            .iter()
            .map(|c| CraterDto {
                name: c.name.into(),
                lat_deg: c.lat,
                lon_deg: c.lon,
                diameter_km: c.diameter_km,
            })
            .collect(),
        maria: MARIA
            .iter()
            .map(|m| MareDto {
                name: m.name.into(),
                lat_deg: m.lat,
                lon_deg: m.lon,
                circular: matches!(m.shape, MareShape::Circle { .. }),
                outline: features::outline(m)
                    .iter()
                    .flat_map(|&(lat, lon)| {
                        [
                            ((lat / DEG * 1e3).round() / 1e3) as f32,
                            ((lon / DEG * 1e3).round() / 1e3) as f32,
                        ]
                    })
                    .collect(),
            })
            .collect(),
        moon_radius_km: features::MOON_RADIUS_KM,
        attribution: "Feature names and positions: IAU Gazetteer of Planetary Nomenclature \
                      (IAU WGPSN / USGS Astrogeology), public domain. Mare outlines are schematic."
            .into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ephemeris::meeus;

    fn params(hour: u32) -> MoonViewParams {
        MoonViewParams {
            instant: UtDateTime {
                year: 2024,
                month: 3,
                day: 11,
                hour,
                minute: 0,
                second: 0.0,
            },
            location: Some(Location {
                lat_deg: 21.4225,
                lon_deg: 39.8262,
                elevation_m: 277.0,
            }),
            ephemeris_id: None,
            crater: Some("copernicus".into()),
            include_track: true,
        }
    }

    #[test]
    fn evening_crescent_at_mecca() {
        // 2024-03-11 16:00 UT: the ~31-hour crescent low in the west after
        // sunset at Mecca (new Moon 2024-03-10 09:00 UT).
        let eph = meeus(false);
        let v = moon_view(&*eph, &params(16)).unwrap();
        assert!(v.topocentric);
        assert_eq!(v.phase_name, "Waxing crescent");
        assert!((1.2..1.4).contains(&v.age_days), "{}", v.age_days);
        assert_eq!((v.last_new_moon.day, v.last_new_moon.hour), (10, 9));
        assert_eq!((v.next_full_moon.month, v.next_full_moon.day), (3, 25));
        assert!((0.01..0.05).contains(&v.illuminated_fraction));
        // Waxing: lit on the west side, χ near 270°; in the zenith frame
        // the lit limb points down-ish toward the set Sun.
        assert!(
            (230.0..310.0).contains(&v.bright_limb_deg),
            "{}",
            v.bright_limb_deg
        );
        let z = v.bright_limb_zenith_deg.unwrap();
        assert!((120.0..240.0).contains(&z), "χ − q = {z}");
        let alt = v.alt_deg.unwrap();
        assert!((0.0..15.0).contains(&alt) && (240.0..300.0).contains(&v.az_deg.unwrap()));
        // Physical ephemeris within the known ranges.
        let g = &v.geocentric;
        assert!(g.libration.lon_deg.abs() < 8.2 && g.libration.lat_deg.abs() < 7.0);
        assert!(
            (g.colongitude_deg - (90.0 - g.sun_selenographic_lon_deg)).rem_euclid(360.0) < 1e-9
        );
        // Near new Moon the colongitude is near 270°.
        assert!(
            (g.colongitude_deg - 280.0).abs() < 20.0,
            "{}",
            g.colongitude_deg
        );
        // Topocentric values differ from geocentric by under ~1.5°.
        let t = v.topocentric_physical.unwrap();
        assert!((t.libration.lon_deg - g.libration.lon_deg).abs() < 1.5);
        assert_eq!(v.disc.l_deg, t.libration.lon_deg);
        assert!(v.disc.zenith_up_rotation_deg.is_some());
        // Diameter about 30′ (closer topocentrically than geocentrically).
        assert!((29.0..34.0).contains(&v.apparent_diameter_arcmin));
        assert!(v.distance_km < v.geocentric_distance_km);
        // Copernicus is in the night until about March 19.
        let c = v.crater.unwrap();
        assert!(c.sun_alt_deg < 0.0);
        assert_eq!(c.next_sunrise.unwrap().day, 19);
        // The rendered disc reproduces χ and k.
        let view = disc::View {
            l: v.disc.l_deg * DEG,
            b: v.disc.b_deg * DEG,
            sun_lon: v.disc.sun_lon_deg * DEG,
            sun_lat: v.disc.sun_lat_deg * DEG,
            rotation: v.disc.north_up_rotation_deg * DEG,
        };
        let screen = norm_2pi(view.bright_limb_screen_angle()) / DEG;
        assert!(
            (screen - v.bright_limb_deg).abs() < 1.5,
            "{screen} vs {}",
            v.bright_limb_deg
        );
        assert!((view.illuminated_fraction() - v.illuminated_fraction).abs() < 0.01);
        // The libration curve of this lunation contains the instant.
        let tr = v.libration_track.unwrap();
        assert_eq!((tr.new_moon.day, tr.new_moon.hour), (10, 9));
        assert!(tr.points.len() > 100);
        assert!((tr.now[0] - g.libration.lon_deg).abs() < 1e-9);
    }

    #[test]
    fn geocentric_and_errors() {
        let eph = meeus(false);
        let mut p = params(0);
        p.location = None;
        p.crater = None;
        p.include_track = false;
        let v = moon_view(&*eph, &p).unwrap();
        assert!(!v.topocentric && v.parallactic_deg.is_none() && v.alt_deg.is_none());
        assert!(v.libration_track.is_none() && v.topocentric_physical.is_none());
        assert_eq!(v.disc.l_deg, v.geocentric.libration.lon_deg);
        p.crater = Some("Nowhere".into());
        assert_eq!(moon_view(&*eph, &p).unwrap_err().code, "invalid-params");
    }

    #[test]
    fn features_are_sent() {
        let f = moon_features();
        assert_eq!(f.craters.len(), CRATERS.len());
        assert!(f.craters.len() >= 30);
        assert!(
            f.maria
                .iter()
                .any(|m| m.name == "Mare Crisium" && m.circular)
        );
        let proc = f
            .maria
            .iter()
            .find(|m| m.name == "Oceanus Procellarum")
            .unwrap();
        assert!(!proc.circular && proc.outline.len() % 2 == 0);
        // Closed outlines.
        let n = proc.outline.len();
        assert_eq!(proc.outline[..2], proc.outline[n - 2..]);
    }

    #[test]
    fn lunar_sunrise_is_next_and_ordered() {
        // Copernicus (9.6° N, 20.1° W) gets the Sun about two days after
        // first quarter: first quarter was 2024 March 17, so sunrise is
        // around March 19.
        let eph = mooncalc_core::MeeusEphemeris::new();
        let jd = Instant::from_calendar(2024, 3, 11, 0, 0, 0.0).jd_tt();
        let (eta, theta) = (-20.1 * DEG, 9.6 * DEG);
        let rise = next_lunar_sun_event(&eph, eta, theta, jd, true).unwrap();
        let set = next_lunar_sun_event(&eph, eta, theta, jd, false).unwrap();
        assert!(rise > jd && rise - jd < SYNODIC_MONTH_DAYS);
        let c = Instant::from_jd_tt(rise).to_calendar();
        assert_eq!((c.month, c.day), (3, 19));
        // Sunset follows about half a lunation later.
        assert!((set - rise - SYNODIC_MONTH_DAYS / 2.0).abs() < 1.5);
    }
}
