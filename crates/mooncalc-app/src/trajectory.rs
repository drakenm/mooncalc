//! The local day of the Sun and Moon, in the spirit of mooncalc.org: their
//! altitude and azimuth every five minutes over one local calendar day,
//! rise, transit and set with azimuths, twilight, and the principal phases
//! around the day for the age and next new/full Moon readouts.
//!
//! | Operation | Input | Output |
//! | --- | --- | --- |
//! | [`trajectory_day`] | [`TrajectoryParams`] | [`TrajectoryDay`] |
//!
//! Computed by [`crate::sky::scene::DayScene`], shared with the egui
//! Trajectory tab. Rise and set use the standard horizon (upper limb, 34′
//! refraction), as the report does; sample altitudes are apparent
//! (refracted centre). Distances, illumination and the elongation that
//! names the phase come per sample, so a frontend can show the readouts at
//! any slider time; the shadow of a vertical object is `h / tan(alt)` (see
//! [`crate::sky::geometry::shadow_length`]).

use mooncalc_core::angle::DEG;
use mooncalc_core::events::phases::Phase;
use mooncalc_core::events::{CalendarDate, Circumpolar, DayEvents};
use mooncalc_core::topocentric::moon_semidiameter;
use mooncalc_core::{Ephemeris, GeoLocation, Instant};
use serde::{Deserialize, Serialize};

use crate::dto::{ApiError, CivilDate, Location, UtDateTime};
use crate::ephemeris::check_covers;
use crate::sky::scene::{DAY_SAMPLE_STEP, DayScene};
use crate::validate;
use crate::zone::Localize;

/// Input of `trajectory_day`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryParams {
    /// The local calendar date, midnight to midnight in the request's zone
    /// (23 or 25 hours when clocks change).
    pub date: CivilDate,
    /// Fixed offset from UT, hours, used when `time_zone` is absent.
    #[serde(default)]
    pub tz_offset_hours: f64,
    /// IANA time zone; takes precedence over `tz_offset_hours`.
    #[serde(default)]
    pub time_zone: Option<String>,
    pub location: Location,
    #[serde(default)]
    pub ephemeris_id: Option<String>,
}

/// The Sun and Moon at one instant of the day.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackSample {
    /// Apparent (refracted) altitude of the centre.
    pub sun_alt_deg: f64,
    pub sun_az_deg: f64,
    pub moon_alt_deg: f64,
    pub moon_az_deg: f64,
    /// Topocentric illuminated fraction, 0..1.
    pub illuminated_fraction: f64,
    /// Geocentric elongation in ecliptic longitude λ☾ − λ☉, `[0, 360)`:
    /// names the phase.
    pub elongation_lon_deg: f64,
    /// Bright limb from the zenith, χ − q (the crescent's tilt).
    pub limb_zenith_deg: f64,
    /// Observer-to-Moon distance, km.
    pub moon_distance_km: f64,
    /// Earth-centre-to-Moon distance, km.
    pub moon_geo_distance_km: f64,
    pub moon_semi_diameter_arcmin: f64,
}

/// Rise, transit and set of one body on the local day.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BodyDayEvents {
    pub rise: Option<UtDateTime>,
    pub set: Option<UtDateTime>,
    /// Upper transit (culmination).
    pub transit: Option<UtDateTime>,
    pub rise_az_deg: Option<f64>,
    pub set_az_deg: Option<f64>,
    /// Apparent altitude at the upper transit.
    pub transit_alt_deg: Option<f64>,
    /// `"always-up"` or `"always-down"` when the body neither rises nor
    /// sets that day.
    pub circumpolar: Option<String>,
}

/// When a twilight begins in the morning and ends in the evening.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryTwilight {
    pub dawn: Option<UtDateTime>,
    pub dusk: Option<UtDateTime>,
}

/// A principal lunar phase.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhaseEvent {
    /// `new`, `first-quarter`, `full` or `last-quarter`.
    pub phase: String,
    pub time: UtDateTime,
}

/// One local day of the Sun and Moon.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryDay {
    pub location: Location,
    pub date: CivilDate,
    /// The offset at local midnight starting the day, hours (times carry
    /// their own).
    pub tz_offset_hours: f64,
    /// The IANA zone used, if any.
    pub time_zone: Option<String>,
    pub ephemeris: String,
    /// Local midnight starting the day; sample `i` is at
    /// `day_start + i × step_minutes`, the last at the next midnight
    /// (`day_end`; 23 or 25 hours later when clocks change).
    pub day_start: UtDateTime,
    pub day_end: UtDateTime,
    pub step_minutes: f64,
    pub samples: Vec<TrackSample>,
    pub sun: BodyDayEvents,
    pub moon: BodyDayEvents,
    /// Sun 6°, 12° and 18° below the horizon.
    pub civil: TrajectoryTwilight,
    pub nautical: TrajectoryTwilight,
    pub astronomical: TrajectoryTwilight,
    /// Principal phases from about a month before the day to two months
    /// after, in time order: the age is counted from the last `new` before
    /// an instant.
    pub phases: Vec<PhaseEvent>,
}

/// Wire name of a phase.
pub fn phase_code(p: Phase) -> &'static str {
    match p {
        Phase::New => "new",
        Phase::FirstQuarter => "first-quarter",
        Phase::Full => "full",
        Phase::LastQuarter => "last-quarter",
    }
}

fn ut(t: Instant) -> UtDateTime {
    t.into()
}

fn body_events(day: &DayScene, ev: &DayEvents, moon: bool) -> BodyDayEvents {
    let at = |t: Option<Instant>| {
        t.map(|t| {
            let s = day.snapshot(t.jd_ut);
            if moon { s.moon } else { s.sun }
        })
    };
    BodyDayEvents {
        rise: ev.rise.map(ut),
        set: ev.set.map(ut),
        transit: ev.upper_transit.map(ut),
        rise_az_deg: at(ev.rise).map(|b| b.az / DEG),
        set_az_deg: at(ev.set).map(|b| b.az / DEG),
        transit_alt_deg: at(ev.upper_transit).map(|b| b.alt / DEG),
        circumpolar: ev.circumpolar.map(|c| {
            match c {
                Circumpolar::AlwaysUp => "always-up",
                Circumpolar::AlwaysDown => "always-down",
            }
            .into()
        }),
    }
}

/// The Sun's and Moon's day at one place.
pub fn trajectory_day(
    eph: &dyn Ephemeris,
    p: &TrajectoryParams,
) -> Result<TrajectoryDay, ApiError> {
    validate::date(p.date)?;
    validate::location(p.location)?;
    let zone = crate::zone::resolve(p.tz_offset_hours, p.time_zone.as_deref())?;
    let date = CalendarDate::from(p.date);
    let start_tt = Instant::from_jd_ut(date.jd0()).jd_tt();
    check_covers(eph, start_tt - 33.0, start_tt + 63.0)?;
    let day = DayScene::compute(eph, date, zone, GeoLocation::from(p.location));
    let samples = day
        .samples
        .iter()
        .map(|s| TrackSample {
            sun_alt_deg: s.sun.alt / DEG,
            sun_az_deg: s.sun.az / DEG,
            moon_alt_deg: s.moon.alt / DEG,
            moon_az_deg: s.moon.az / DEG,
            illuminated_fraction: s.illumination.fraction,
            elongation_lon_deg: s.elongation_lon_deg,
            limb_zenith_deg: s.limb_zenith / DEG,
            moon_distance_km: s.moon.topo.distance_km,
            moon_geo_distance_km: s.moon_geo_distance_km,
            moon_semi_diameter_arcmin: moon_semidiameter(s.moon.topo.distance_km) / DEG * 60.0,
        })
        .collect();
    let tw = |i: usize| TrajectoryTwilight {
        dawn: day.twilights[i].0.map(ut),
        dusk: day.twilights[i].1.map(ut),
    };
    let mut result = TrajectoryDay {
        location: p.location,
        date: p.date,
        tz_offset_hours: zone.offset_hours(day.start),
        time_zone: zone.iana_name().map(str::to_owned),
        ephemeris: day.ephemeris_name.clone(),
        day_start: ut(Instant::from_jd_ut(day.start)),
        day_end: ut(Instant::from_jd_ut(day.end)),
        step_minutes: DAY_SAMPLE_STEP * 1440.0,
        samples,
        sun: body_events(&day, &day.sun, false),
        moon: body_events(&day, &day.moon, true),
        civil: tw(0),
        nautical: tw(1),
        astronomical: tw(2),
        phases: day
            .phases
            .iter()
            .map(|(ph, t)| PhaseEvent {
                phase: phase_code(*ph).into(),
                time: ut(*t),
            })
            .collect(),
    };
    result.localize(zone);
    Ok(result)
}

crate::zone::localize_fields! {
    BodyDayEvents: rise, set, transit;
    TrajectoryTwilight: dawn, dusk;
    PhaseEvent: time;
    TrajectoryDay: day_start, day_end, sun, moon, civil, nautical, astronomical, phases;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ephemeris::meeus;

    fn hours(day: &TrajectoryDay, t: &UtDateTime) -> f64 {
        (t.to_instant().jd_ut - day.day_start.to_instant().jd_ut) * 24.0
    }

    #[test]
    fn london_2024_03_11() {
        let eph = meeus(false);
        let d = trajectory_day(
            &*eph,
            &TrajectoryParams {
                date: CivilDate {
                    year: 2024,
                    month: 3,
                    day: 11,
                },
                tz_offset_hours: 0.0,
                time_zone: None,
                location: Location {
                    lat_deg: 51.5074,
                    lon_deg: -0.1278,
                    elevation_m: 11.0,
                },
                ephemeris_id: None,
            },
        )
        .unwrap();
        assert_eq!(d.samples.len(), 289);
        assert_eq!(d.step_minutes, 5.0);
        assert_eq!((d.day_start.day, d.day_start.hour), (11, 0));
        // Sunrise 06:19, sunset 18:00 UT (timeanddate.com), near due east
        // and west at the equinox: azimuths about 96° and 264°.
        assert!((hours(&d, &d.sun.rise.unwrap()) - 6.33).abs() < 0.1);
        assert!((hours(&d, &d.sun.set.unwrap()) - 18.0).abs() < 0.1);
        let (ra, sa) = (d.sun.rise_az_deg.unwrap(), d.sun.set_az_deg.unwrap());
        assert!(
            (92.0..100.0).contains(&ra) && (260.0..268.0).contains(&sa),
            "{ra} {sa}"
        );
        // Solar noon about 12:10 UT; the Sun at δ ≈ −3.4° culminates at
        // 90° − 51.5° − 3.4° ≈ 35.1° (plus 0.02° refraction).
        let ta = d.sun.transit_alt_deg.unwrap();
        assert!((ta - 35.1).abs() < 0.15, "{ta}");
        assert!(d.sun.circumpolar.is_none());
        // The young Moon rises after the Sun, north of it, and sets after it.
        let mr = hours(&d, &d.moon.rise.unwrap());
        let ms = hours(&d, &d.moon.set.unwrap());
        assert!(mr > 6.3 && mr < 9.0 && ms > 19.0 && ms < 23.0, "{mr} {ms}");
        // Twilight in order around sunset.
        let s = hours(&d, &d.sun.set.unwrap());
        let c = hours(&d, &d.civil.dusk.unwrap());
        let n = hours(&d, &d.nautical.dusk.unwrap());
        let a = hours(&d, &d.astronomical.dusk.unwrap());
        assert!(s < c && c < n && n < a);
        // Phases: the new Moon of 2024-03-10 09:00 UT, then the full Moon
        // of 2024-03-25 07:00 UT.
        let new = d
            .phases
            .iter()
            .rev()
            .find(|p| p.phase == "new" && hours(&d, &p.time) < 0.0)
            .unwrap();
        assert_eq!((new.time.day, new.time.hour), (10, 9));
        let full = d
            .phases
            .iter()
            .find(|p| p.phase == "full" && hours(&d, &p.time) > 0.0)
            .unwrap();
        assert_eq!((full.time.month, full.time.day, full.time.hour), (3, 25, 7));
        // Noon readouts: about 1.1 days old, ~4 % lit, waxing crescent.
        let noon = &d.samples[144];
        assert!(noon.illuminated_fraction < 0.07 && noon.elongation_lon_deg < 30.0);
        assert!((356_000.0..407_000.0).contains(&noon.moon_geo_distance_km));
    }

    #[test]
    fn polar_day_and_bad_input() {
        let eph = meeus(false);
        let mut p = TrajectoryParams {
            date: CivilDate {
                year: 2026,
                month: 6,
                day: 21,
            },
            tz_offset_hours: 1.0,
            time_zone: None,
            location: Location {
                lat_deg: 78.2,
                lon_deg: 15.6,
                elevation_m: 0.0,
            },
            ephemeris_id: None,
        };
        let d = trajectory_day(&*eph, &p).unwrap();
        assert_eq!(d.sun.circumpolar.as_deref(), Some("always-up"));
        assert!(d.sun.rise.is_none() && d.civil.dusk.is_none());
        p.tz_offset_hours = 20.0;
        assert!(trajectory_day(&*eph, &p).is_err());
    }
}
