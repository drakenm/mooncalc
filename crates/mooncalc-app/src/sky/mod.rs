//! The sky of a crescent sighting: Sun and Moon tracks over the evening
//! (or morning), the crescent's phase and orientation, ARCV/DAZ, and the
//! bright stars of date.
//!
//! | Operation | Input | Output |
//! | --- | --- | --- |
//! | [`sky_scene`] | [`SkySceneParams`] | [`SkyScene`] |
//! | [`sky_stars`] | [`SkyStarsParams`] | [`SkyStars`] |
//!
//! The scene is sampled every two minutes from an hour before the slider
//! span (sunset → moonset, or moonrise → sunrise) to an hour after it, so a
//! frontend animates the time slider by interpolating between samples and
//! never calls back per frame. Stars come as right ascension and
//! declination of date; a frontend turns them into altitude and azimuth
//! with each sample's local sidereal time (or asks [`sky_stars`] for one
//! instant).
//!
//! The submodules are the frontend-independent machinery shared with the
//! egui Sky and Trajectory tabs: [`geometry`] (parallactic angle, bright
//! limb from the zenith, crescent outline, phase and compass names),
//! [`scene`] (topocentric snapshots, [`scene::NightScene`],
//! [`scene::DayScene`]) and [`stars`] (Yale BSC5, V < 3.5, reduced to
//! date).

pub mod geometry;
pub mod scene;
pub mod stars;
mod stars_data;

use mooncalc_core::angle::{DEG, norm_pi};
use mooncalc_core::events::CalendarDate;
use mooncalc_core::nutation::NutationObliquity;
use mooncalc_core::refraction::apparent_altitude;
use mooncalc_core::topocentric::{moon_semidiameter, sun_semidiameter};
use mooncalc_core::transform::equatorial_to_horizontal;
use mooncalc_core::{Ephemeris, Equatorial, GeoLocation, Instant};
use mooncalc_visibility::Criterion;
use mooncalc_visibility::criteria::CriterionResult;
use serde::{Deserialize, Serialize};

use crate::criteria::evaluation_time_text;
use crate::dto::{
    ApiError, CivilDate, Crescent, CrescentGeometryDto, Location, UtDateTime, Verdict,
};
use crate::ephemeris::check_covers;
use crate::report::{status_code, status_text};
use crate::validate;
use scene::{ATMOSPHERE, NIGHT_SAMPLE_STEP, NightScene, Snapshot};
use stars::{BRIGHT_STARS, StarOfDate, stars_of_date};

/// Criterion used when a request names none.
pub const DEFAULT_CRITERION: &str = "yallop";

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

/// Input of `sky_scene`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkySceneParams {
    /// The local calendar date (in the `tz_offset_hours` zone) whose evening
    /// (or morning) is shown; civil noon anchors it, as in the report.
    pub date: CivilDate,
    pub tz_offset_hours: f64,
    pub location: Location,
    pub crescent: Crescent,
    /// Criterion whose evaluation time is the "best time" and whose verdict
    /// is given (default `yallop`).
    #[serde(default)]
    pub criterion_id: Option<String>,
    #[serde(default)]
    pub ephemeris_id: Option<String>,
    /// Faintest star magnitude to send (default 3.5: every embedded star).
    #[serde(default)]
    pub magnitude_limit: Option<f64>,
}

/// The Sun and Moon at one instant of the scene, seen by the observer.
///
/// Altitudes are apparent (refracted, standard atmosphere), azimuths from
/// north through east, RA/Dec topocentric apparent of date. Position
/// angles are counter-clockwise on the sky (from north through east, or
/// from the zenith).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkySample {
    pub sun_alt_deg: f64,
    pub sun_az_deg: f64,
    pub moon_alt_deg: f64,
    pub moon_az_deg: f64,
    pub sun_ra_deg: f64,
    pub sun_dec_deg: f64,
    pub moon_ra_deg: f64,
    pub moon_dec_deg: f64,
    /// Local apparent sidereal time.
    pub lst_deg: f64,
    /// Topocentric illuminated fraction of the Moon, 0..1.
    pub illuminated_fraction: f64,
    /// Position angle χ of the bright limb from north (Meeus eq. 48.5).
    pub bright_limb_deg: f64,
    /// Parallactic angle q of the Moon (Meeus eq. 14.1).
    pub parallactic_deg: f64,
    /// Position angle of the bright limb from the zenith, χ − q: the
    /// crescent's orientation in a horizon view.
    pub limb_zenith_deg: f64,
    pub moon_semi_diameter_arcmin: f64,
    pub sun_semi_diameter_arcmin: f64,
    /// Observer-to-Moon distance, km.
    pub moon_distance_km: f64,
    /// Arc of light (topocentric elongation).
    pub arcl_deg: f64,
    /// Arc of vision: the criterion convention of `CrescentGeometry`
    /// (geocentric Moon, airless) when there is a crescent, else the
    /// airless topocentric altitude difference.
    pub arcv_deg: f64,
    /// Relative azimuth, Sun minus Moon.
    pub daz_deg: f64,
    /// Topocentric crescent width, arcminutes, when there is a crescent.
    pub width_arcmin: Option<f64>,
}

/// A star of date. `alt_deg`/`az_deg` are filled in by [`sky_stars`] when
/// a location is given.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StarDto {
    /// Harvard Revised number.
    pub hr: u16,
    /// IAU proper name, or empty.
    pub name: String,
    /// Bayer or Flamsteed designation, e.g. "α CMa", or empty.
    pub label: String,
    pub vmag: f64,
    pub bv: f64,
    /// Right ascension, true equator and equinox of date.
    pub ra_deg: f64,
    pub dec_deg: f64,
    pub alt_deg: Option<f64>,
    pub az_deg: Option<f64>,
}

/// The evening (or morning) sky of a crescent sighting.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkyScene {
    pub location: Location,
    pub date: CivilDate,
    pub tz_offset_hours: f64,
    pub crescent: Crescent,
    pub ephemeris: String,
    pub criterion_id: String,
    pub criterion_name: String,
    /// The criterion's evaluation-time rule, e.g. "Best time (sunset + 4/9 lag)".
    pub evaluation_time: String,
    /// Circumstance status, as `CrescentReport::status`.
    pub status: String,
    pub status_text: String,
    /// Sunset (evening) or sunrise (morning).
    pub sun_event: Option<UtDateTime>,
    /// Moonset (evening) or moonrise (morning).
    pub moon_event: Option<UtDateTime>,
    /// The criterion's evaluation instant.
    pub best_time: Option<UtDateTime>,
    pub conjunction: Option<UtDateTime>,
    /// The slider span and its suggested start (the best time, else the
    /// Sun event).
    pub start: UtDateTime,
    pub end: UtDateTime,
    pub default_time: UtDateTime,
    /// Circumstances at the best time.
    pub geometry: Option<CrescentGeometryDto>,
    /// The criterion's verdict at its evaluation time.
    pub verdict: Option<Verdict>,
    /// Instant of `samples[0]`; sample `i` is `samples_start + i × step_minutes`.
    pub samples_start: UtDateTime,
    pub step_minutes: f64,
    pub samples: Vec<SkySample>,
    /// Bright stars of date (civil noon), brightest first.
    pub stars: Vec<StarDto>,
    /// True obliquity of the ecliptic, for drawing the ecliptic.
    pub obliquity_deg: f64,
}

/// Input of `sky_stars`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkyStarsParams {
    pub instant: UtDateTime,
    /// Observer, for altitude and azimuth; none gives RA/Dec only.
    #[serde(default)]
    pub location: Option<Location>,
    /// Faintest magnitude (default 3.5).
    #[serde(default)]
    pub magnitude_limit: Option<f64>,
}

/// Bright stars at one instant.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkyStars {
    pub instant: UtDateTime,
    /// Local apparent sidereal time, when a location is given.
    pub lst_deg: Option<f64>,
    pub obliquity_deg: f64,
    pub stars: Vec<StarDto>,
}

// ---------------------------------------------------------------------------
// Services
// ---------------------------------------------------------------------------

fn deg(rad: f64) -> f64 {
    rad / DEG
}

fn ut(jd_ut: f64) -> UtDateTime {
    Instant::from_jd_ut(jd_ut).into()
}

pub(crate) fn verdict_dto(c: &dyn Criterion, r: &CriterionResult) -> Verdict {
    let z = &c.zones()[r.zone];
    Verdict {
        zone: r.zone as u8,
        code: z.code.into(),
        description: z.description.into(),
        rgb: z.rgb,
        value: r.value,
    }
}

fn star_dto(s: &StarOfDate) -> StarDto {
    StarDto {
        hr: s.star.hr,
        name: s.star.name.into(),
        label: s.star.label.into(),
        vmag: s.star.vmag,
        bv: s.star.bv,
        ra_deg: deg(s.ra).rem_euclid(360.0),
        dec_deg: deg(s.dec),
        alt_deg: None,
        az_deg: None,
    }
}

fn magnitude_limit(m: Option<f64>) -> Result<f64, ApiError> {
    let m = m.unwrap_or(3.5);
    if !m.is_finite() {
        return Err(ApiError::invalid_params("magnitudeLimit must be a number"));
    }
    Ok(m)
}

fn sample(night: &NightScene, s: &Snapshot) -> SkySample {
    let g = night.geometry(s.jd_ut);
    SkySample {
        sun_alt_deg: deg(s.sun.alt),
        sun_az_deg: deg(s.sun.az),
        moon_alt_deg: deg(s.moon.alt),
        moon_az_deg: deg(s.moon.az),
        sun_ra_deg: deg(s.sun.topo.ra).rem_euclid(360.0),
        sun_dec_deg: deg(s.sun.topo.dec),
        moon_ra_deg: deg(s.moon.topo.ra).rem_euclid(360.0),
        moon_dec_deg: deg(s.moon.topo.dec),
        lst_deg: deg(s.lst).rem_euclid(360.0),
        illuminated_fraction: s.illumination.fraction,
        bright_limb_deg: deg(s.illumination.bright_limb_angle),
        parallactic_deg: deg(s.parallactic),
        limb_zenith_deg: deg(s.limb_zenith),
        moon_semi_diameter_arcmin: deg(moon_semidiameter(s.moon.topo.distance_km)) * 60.0,
        sun_semi_diameter_arcmin: deg(sun_semidiameter(s.sun.topo.distance_km)) * 60.0,
        moon_distance_km: s.moon.topo.distance_km,
        arcl_deg: deg(s.illumination.elongation),
        arcv_deg: g.map_or(deg(s.moon.alt_airless - s.sun.alt_airless), |g| g.arcv_deg),
        daz_deg: g.map_or(deg(norm_pi(s.sun.az - s.moon.az)), |g| g.daz_deg),
        width_arcmin: g.map(|g| g.width_topo_arcmin),
    }
}

/// The evening (or morning) sky of a crescent sighting: samples of the Sun
/// and Moon every two minutes, events, the best-time geometry and verdict,
/// and the bright stars of date.
pub fn sky_scene(eph: &dyn Ephemeris, p: &SkySceneParams) -> Result<SkyScene, ApiError> {
    validate::date(p.date)?;
    validate::location(p.location)?;
    validate::tz_offset(p.tz_offset_hours)?;
    let mag = magnitude_limit(p.magnitude_limit)?;
    let criterion_id = p.criterion_id.as_deref().unwrap_or(DEFAULT_CRITERION);
    let criterion = mooncalc_visibility::criteria::by_id(criterion_id)
        .ok_or_else(|| ApiError::unknown_criterion(criterion_id))?;
    let date = CalendarDate::from(p.date);
    let noon = scene::civil_noon(date, p.tz_offset_hours);
    let noon_tt = Instant::from_jd_ut(noon).jd_tt();
    check_covers(eph, noon_tt - 32.0, noon_tt + 32.0)?;

    let night = NightScene::compute(
        eph,
        date,
        p.tz_offset_hours,
        GeoLocation::from(p.location),
        p.crescent.into(),
        criterion_id,
    );
    let circ = night.outcome.circumstances();
    let samples = night.samples.iter().map(|s| sample(&night, s)).collect();
    let stars = night
        .stars
        .iter()
        .filter(|s| s.star.vmag <= mag)
        .map(star_dto)
        .collect();
    Ok(SkyScene {
        location: p.location,
        date: p.date,
        tz_offset_hours: p.tz_offset_hours,
        crescent: p.crescent,
        ephemeris: night.ephemeris_name.clone(),
        criterion_id: criterion.id().into(),
        criterion_name: criterion.name().into(),
        evaluation_time: evaluation_time_text(criterion.evaluation_time()),
        status: status_code(night.outcome.status()).into(),
        status_text: status_text(night.outcome.status(), p.crescent),
        sun_event: night.sun_event.map(ut),
        moon_event: night.moon_event.map(ut),
        best_time: night.evaluation.map(ut),
        conjunction: circ.map(|c| c.conjunction.into()),
        start: ut(night.start),
        end: ut(night.end),
        default_time: ut(night.default_time()),
        geometry: circ.map(|c| CrescentGeometryDto::from(&c.geometry)),
        verdict: night
            .verdict
            .as_ref()
            .map(|r| verdict_dto(criterion.as_ref(), r)),
        samples_start: ut(night.samples[0].jd_ut),
        step_minutes: NIGHT_SAMPLE_STEP * 1440.0,
        samples,
        stars,
        obliquity_deg: deg(NutationObliquity::at(noon_tt).eps),
    })
}

/// The bright stars reduced to the true equator and equinox of an instant,
/// with apparent (refracted) altitude and azimuth when a location is given.
pub fn sky_stars(p: &SkyStarsParams) -> Result<SkyStars, ApiError> {
    validate::date(p.instant.date())?;
    if let Some(l) = p.location {
        validate::location(l)?;
    }
    let mag = magnitude_limit(p.magnitude_limit)?;
    let t = p.instant.to_instant();
    let jd_tt = t.jd_tt();
    let lst = p
        .location
        .map(|l| t.local_apparent_sidereal_time(l.lon_deg * DEG));
    let stars = stars_of_date(jd_tt)
        .iter()
        .filter(|s| s.star.vmag <= mag)
        .map(|s| {
            let mut d = star_dto(s);
            if let (Some(l), Some(lst)) = (p.location, lst) {
                let eq = Equatorial {
                    ra: s.ra,
                    dec: s.dec,
                    distance_km: 1.0,
                };
                let h = equatorial_to_horizontal(&eq, l.lat_deg * DEG, lst);
                d.alt_deg = Some(deg(apparent_altitude(h.alt, &ATMOSPHERE)));
                d.az_deg = Some(deg(h.az).rem_euclid(360.0));
            }
            d
        })
        .collect();
    debug_assert!(BRIGHT_STARS.len() >= 250);
    Ok(SkyStars {
        instant: p.instant,
        lst_deg: lst.map(|v| deg(v).rem_euclid(360.0)),
        obliquity_deg: deg(NutationObliquity::at(jd_tt).eps),
        stars,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ephemeris::meeus;

    fn mecca_params() -> SkySceneParams {
        SkySceneParams {
            date: CivilDate {
                year: 2024,
                month: 3,
                day: 11,
            },
            tz_offset_hours: 3.0,
            location: Location {
                lat_deg: 21.4225,
                lon_deg: 39.8262,
                elevation_m: 277.0,
            },
            crescent: Crescent::Evening,
            criterion_id: None,
            ephemeris_id: None,
            magnitude_limit: None,
        }
    }

    fn minutes(a: &UtDateTime, b: &UtDateTime) -> f64 {
        (b.to_instant().jd_ut - a.to_instant().jd_ut) * 1440.0
    }

    #[test]
    fn young_crescent_at_mecca() {
        // New Moon 2024 March 10, 09:00 UT; on the evening of the 11th the
        // crescent is about 30 h old and easily visible from Mecca.
        let eph = meeus(false);
        let s = sky_scene(&*eph, &mecca_params()).unwrap();
        assert_eq!(s.status, "crescent");
        assert_eq!(s.criterion_id, "yallop");
        assert_eq!(s.verdict.as_ref().unwrap().code, "A");
        let (ss, ms) = (s.sun_event.unwrap(), s.moon_event.unwrap());
        // Sunset about 15:29 UT (18:29 local; timeanddate.com 18:29).
        let h = ss.hour as f64 + ss.minute as f64 / 60.0;
        assert!((15.3..15.7).contains(&h), "sunset {h} h UT");
        assert!(
            (60.0..95.0).contains(&minutes(&ss, &ms)),
            "lag {}",
            minutes(&ss, &ms)
        );
        assert!(minutes(&s.start, &ss) > 0.0 && minutes(&ms, &s.end) > 0.0);
        let best = s.best_time.unwrap();
        assert_eq!(s.default_time, best);
        // Samples cover the span every two minutes.
        assert_eq!(s.step_minutes, 2.0);
        let n = s.samples.len() as f64;
        assert!(minutes(&s.samples_start, &s.start) >= 59.0);
        assert!((n - 1.0) * 2.0 >= minutes(&s.samples_start, &s.end) + 59.0);
        // At the best time: thin crescent above the western horizon, the
        // Sun below, the lit limb pointing down toward the Sun.
        let i = (minutes(&s.samples_start, &best) / 2.0).round() as usize;
        let b = &s.samples[i];
        assert!(b.illuminated_fraction < 0.06 && b.illuminated_fraction > 0.01);
        assert!(b.moon_alt_deg > 0.0 && b.sun_alt_deg < 0.0);
        assert!((240.0..300.0).contains(&b.moon_az_deg));
        assert!((120.0..240.0).contains(&b.limb_zenith_deg));
        assert!(
            (b.limb_zenith_deg - (b.bright_limb_deg - b.parallactic_deg).rem_euclid(360.0)).abs()
                < 1e-9
        );
        // ARCV/DAZ near the best time agree with the reported geometry.
        let g = s.geometry.unwrap();
        assert!(
            (b.arcv_deg - g.arcv_deg).abs() < 0.6,
            "{} {}",
            b.arcv_deg,
            g.arcv_deg
        );
        assert!((b.daz_deg - g.daz_deg).abs() < 0.6);
        assert!(b.width_arcmin.is_some());
        assert!((14.0..17.5).contains(&b.moon_semi_diameter_arcmin));
        assert!((15.5..16.5).contains(&b.sun_semi_diameter_arcmin));
        // Every embedded star, brightest first, of date.
        assert_eq!(s.stars.len(), BRIGHT_STARS.len());
        assert_eq!(s.stars[0].name, "Sirius");
        assert!((s.obliquity_deg - 23.44).abs() < 0.01);
    }

    #[test]
    fn morning_and_errors() {
        let eph = meeus(false);
        let mut p = mecca_params();
        p.date.day = 8;
        p.crescent = Crescent::Morning;
        p.criterion_id = Some("odeh".into());
        p.magnitude_limit = Some(1.0);
        let s = sky_scene(&*eph, &p).unwrap();
        assert_eq!(s.status, "crescent");
        assert_eq!(s.criterion_id, "odeh");
        let (sr, mr) = (s.sun_event.unwrap(), s.moon_event.unwrap());
        assert!(minutes(&mr, &sr) > 0.0);
        assert!(s.stars.iter().all(|st| st.vmag <= 1.0) && s.stars.len() < 25);
        p.criterion_id = Some("nope".into());
        assert_eq!(sky_scene(&*eph, &p).unwrap_err().code, "unknown-criterion");
        p.criterion_id = None;
        p.location.lat_deg = 95.0;
        assert_eq!(sky_scene(&*eph, &p).unwrap_err().code, "invalid-params");
    }

    #[test]
    fn stars_at_an_instant() {
        // London, 2024-03-11 21:00 UT: Sirius low in the SSW, Polaris at
        // the latitude's altitude in the north, Vega low in the north.
        let r = sky_stars(&SkyStarsParams {
            instant: UtDateTime {
                year: 2024,
                month: 3,
                day: 11,
                hour: 21,
                minute: 0,
                second: 0.0,
            },
            location: Some(Location {
                lat_deg: 51.5074,
                lon_deg: -0.1278,
                elevation_m: 11.0,
            }),
            magnitude_limit: None,
        })
        .unwrap();
        let star = |n: &str| r.stars.iter().find(|s| s.name == n).unwrap();
        let sirius = star("Sirius");
        let (alt, az) = (sirius.alt_deg.unwrap(), sirius.az_deg.unwrap());
        assert!(
            (10.0..25.0).contains(&alt) && (190.0..235.0).contains(&az),
            "{alt} {az}"
        );
        let polaris = star("Polaris");
        assert!((polaris.alt_deg.unwrap() - 51.5).abs() < 1.0);
        assert!(polaris.az_deg.unwrap() < 2.0 || polaris.az_deg.unwrap() > 358.0);
        // LST at 21:00 UT on March 11 is about 8.4 h.
        assert!(
            (r.lst_deg.unwrap() / 15.0 - 8.4).abs() < 0.1,
            "{:?}",
            r.lst_deg
        );
        // Without a location, RA/Dec only.
        let r = sky_stars(&SkyStarsParams {
            location: None,
            magnitude_limit: Some(0.5),
            ..SkyStarsParams {
                instant: r.instant,
                location: None,
                magnitude_limit: None,
            }
        })
        .unwrap();
        assert!(r.lst_deg.is_none() && r.stars.iter().all(|s| s.alt_deg.is_none()));
        assert!(r.stars.len() < 15);
    }
}
