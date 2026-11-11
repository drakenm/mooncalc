//! The location report: rise/set and twilight for a local date, and the
//! evening and morning crescents under every criterion.
//!
//! The day is anchored at **civil noon** of the local date in the given
//! time zone (`mooncalc_visibility::circumstances::Request::noon`), the
//! convention of HMNAO's city tables. The evening crescent follows that
//! noon, the morning one precedes it. Rise and set use the standard horizon
//! (upper limb, 34′ refraction, level horizon), as HMNAO and USNO do.
//!
//! World maps put the date line on the 180° meridian instead (each place on
//! its local-mean-time day). Where the civil date differs from that, east of
//! 180° in UTC+13/+14, the report carries a
//! [`LocationReport::map_date_note`] naming the map date that shows its
//! evening ([`crate::dateline::report_note`]).
//!
//! Each criterion is evaluated at its own evaluation time (Yallop's best
//! time, sunset, or a solar altitude), so the geometry behind each verdict is
//! that criterion's. The report's headline geometry is at Yallop's best time.

use mooncalc_core::Ephemeris;
use mooncalc_core::GeoLocation;
use mooncalc_core::events::{
    ApparentTable, Body, CalendarDate, Horizon, PositionSource, Twilight, day_events, twilight,
};
use mooncalc_core::illumination::Illumination;
use mooncalc_core::time::Instant;
use mooncalc_visibility::Criterion;
use mooncalc_visibility::circumstances::{
    Outcome, Request, Settings, Sighting, Status, compute_with, new_moons_around,
};
use mooncalc_visibility::criteria::EvaluationTime;

use crate::criteria;
use crate::dto::{
    ApiError, Crescent, CrescentGeometryDto, CrescentReport, CriterionVerdict, DayEventsDto,
    LocationReport, LocationReportParams, UtDateTime, Verdict,
};
use crate::ephemeris::check_covers;
use crate::validate;

/// Ephemeris coverage a report needs either side of civil noon, days: the
/// nearest new Moon and one lunation either side are searched.
pub const COVERAGE_MARGIN_DAYS: f64 = 32.0;

/// Kebab-case status code.
pub fn status_code(s: Status) -> &'static str {
    match s {
        Status::Crescent => "crescent",
        Status::BeforeConjunction => "before-conjunction",
        Status::MoonSetsFirst => "moon-sets-first",
        Status::MoonAlwaysUp => "moon-always-up",
        Status::MoonAlwaysDown => "moon-always-down",
        Status::MidnightSun => "midnight-sun",
        Status::PolarNight => "polar-night",
        Status::SunAltitudeNotReached => "sun-altitude-not-reached",
        Status::OutsideEphemeris => "outside-ephemeris",
    }
}

/// The status for a code of [`status_code`].
pub fn status_from_code(code: &str) -> Option<Status> {
    Status::ALL
        .iter()
        .copied()
        .find(|&s| status_code(s) == code)
}

pub(crate) fn status_text(s: Status, crescent: Crescent) -> String {
    let evening = crescent == Crescent::Evening;
    match s {
        Status::Crescent => {
            if evening {
                "The Moon sets after the Sun, after conjunction: there is a young crescent to look for."
            } else {
                "The Moon rises before the Sun, before conjunction: there is an old crescent to look for."
            }
        }
        Status::BeforeConjunction => {
            if evening {
                "The Moon sets after the Sun, but the best time is before conjunction: no crescent."
            } else {
                "The Moon rises before the Sun, but the best time is after conjunction: no crescent."
            }
        }
        Status::MoonSetsFirst => {
            if evening {
                "The Moon sets before the Sun: no crescent to see."
            } else {
                "The Moon rises after the Sun: no crescent to see."
            }
        }
        Status::MoonAlwaysUp => "The Moon stays above the horizon around the Sun event.",
        Status::MoonAlwaysDown => "The Moon stays below the horizon around the Sun event.",
        Status::MidnightSun => "The Sun does not set (midnight Sun).",
        Status::PolarNight => "The Sun does not rise (polar night).",
        Status::SunAltitudeNotReached => "The Sun does not reach the criterion's altitude.",
        Status::OutsideEphemeris => "The date is outside the ephemeris coverage.",
    }
    .into()
}

fn ut(t: Instant) -> UtDateTime {
    t.into()
}

/// Outcomes per evaluation-time rule, computed on demand.
struct OutcomeCache<'a, S: PositionSource + ?Sized> {
    src: &'a S,
    new_moons: &'a [Instant],
    location: GeoLocation,
    noon: Instant,
    sighting: Sighting,
    settings: Settings,
    done: Vec<(EvaluationTime, Outcome)>,
}

impl<S: PositionSource + ?Sized> OutcomeCache<'_, S> {
    fn get(&mut self, evaluation: EvaluationTime) -> Outcome {
        if let Some((_, o)) = self.done.iter().find(|(e, _)| *e == evaluation) {
            return *o;
        }
        let req = Request {
            location: self.location,
            noon: self.noon,
            sighting: self.sighting,
            evaluation,
        };
        let o = compute_with(self.src, self.new_moons, &req, &self.settings);
        self.done.push((evaluation, o));
        o
    }
}

fn verdict(c: &dyn Criterion, o: &Outcome) -> CriterionVerdict {
    let v = o.evaluate(c).map(|r| {
        let z = &c.zones()[r.zone];
        Verdict {
            zone: r.zone as u8,
            code: z.code.into(),
            description: z.description.into(),
            rgb: z.rgb,
            value: r.value,
        }
    });
    CriterionVerdict {
        criterion_id: c.id().into(),
        name: c.name().into(),
        evaluation_time: criteria::evaluation_time_text(c.evaluation_time()),
        evaluated_at: o.circumstances().map(|c| ut(c.evaluation)),
        status: status_code(o.status()).into(),
        verdict: v,
    }
}

fn crescent_report<S: PositionSource + ?Sized>(
    cache: &mut OutcomeCache<'_, S>,
    crescent: Crescent,
    nearest_new_moon: Instant,
) -> CrescentReport {
    let main = cache.get(EvaluationTime::YallopBestTime);
    let sun_event = match main {
        Outcome::MoonAlwaysUp { sun_event, .. }
        | Outcome::MoonAlwaysDown { sun_event }
        | Outcome::SunAltitudeNotReached { sun_event } => Some(sun_event),
        _ => main.circumstances().map(|c| c.sun_event),
    };
    // Yallop's best time is undefined when the Moon does not set (rise)
    // near the Sun event; the circumstances at the Sun event, which the
    // sunset criteria use, are tabulated instead.
    let (circ, at_sun_event) = match main.circumstances() {
        Some(c) => (Some(*c), false),
        None => {
            let c = cache.get(EvaluationTime::Sunset).circumstances().copied();
            (c, c.is_some())
        }
    };
    let best_time = match main {
        Outcome::Crescent(c) | Outcome::BeforeConjunction(c) => Some(ut(c.evaluation)),
        _ => None,
    };
    let criteria = criteria::all()
        .iter()
        .map(|c| {
            let o = cache.get(c.evaluation_time());
            verdict(&**c, &o)
        })
        .collect();
    CrescentReport {
        crescent,
        status: status_code(main.status()).into(),
        status_text: status_text(main.status(), crescent),
        sun_event: sun_event.map(ut),
        moon_event: circ.and_then(|c| c.moon_event).map(ut),
        best_time,
        conjunction: Some(ut(circ.map_or(nearest_new_moon, |c| c.conjunction))),
        geometry: circ.map(|c| CrescentGeometryDto::from(&c.geometry)),
        geometry_at: circ.map(|c| ut(c.evaluation)),
        geometry_at_sun_event: at_sun_event,
        criteria,
    }
}

/// The full report for one place and local date.
pub fn location_report(
    eph: &dyn Ephemeris,
    p: &LocationReportParams,
) -> Result<LocationReport, ApiError> {
    validate::date(p.date)?;
    validate::location(p.location)?;
    validate::tz_offset(p.tz_offset_hours)?;
    let date = CalendarDate::from(p.date);
    let noon = Instant::from_jd_ut(date.jd0() + 0.5 - p.tz_offset_hours / 24.0);
    check_covers(
        eph,
        noon.jd_tt() - COVERAGE_MARGIN_DAYS,
        noon.jd_tt() + COVERAGE_MARGIN_DAYS,
    )?;

    let location = GeoLocation::from(p.location);
    let table = ApparentTable::with_default_step(eph, noon.add_days(-2.5), noon.add_days(2.5));
    let new_moons = new_moons_around(eph, noon);
    let settings = Settings::default();

    // Rise, set and twilight on the local civil day.
    let (t0, t1) = date.local_day(p.tz_offset_hours);
    let sun = day_events(&table, Body::Sun, &location, Horizon::STANDARD, t0, t1);
    let moon = day_events(&table, Body::Moon, &location, Horizon::STANDARD, t0, t1);
    let (civil_dawn, civil_dusk) = twilight(&table, &location, Twilight::Civil, t0, t1);
    let (nautical_dawn, nautical_dusk) = twilight(&table, &location, Twilight::Nautical, t0, t1);
    let day = DayEventsDto {
        sunrise: sun.rise.map(ut),
        sunset: sun.set.map(ut),
        solar_noon: sun.upper_transit.map(ut),
        moonrise: moon.rise.map(ut),
        moonset: moon.set.map(ut),
        moon_transit: moon.upper_transit.map(ut),
        civil_dawn: civil_dawn.map(ut),
        civil_dusk: civil_dusk.map(ut),
        nautical_dawn: nautical_dawn.map(ut),
        nautical_dusk: nautical_dusk.map(ut),
    };

    // Geocentric age and illumination at civil noon.
    let conj = new_moons
        .iter()
        .min_by(|a, b| {
            (a.jd_ut - noon.jd_ut)
                .abs()
                .total_cmp(&(b.jd_ut - noon.jd_ut).abs())
        })
        .expect("three new Moons");
    let jd_tt = noon.jd_tt();
    let illum = Illumination::compute(&eph.sun_equatorial(jd_tt), &eph.moon_equatorial(jd_tt));

    let cache = |sighting| OutcomeCache {
        src: &table,
        new_moons: &new_moons,
        location,
        noon,
        sighting,
        settings,
        done: Vec::new(),
    };
    let evening = crescent_report(&mut cache(Sighting::Evening), Crescent::Evening, *conj);
    let morning = crescent_report(&mut cache(Sighting::Morning), Crescent::Morning, *conj);

    Ok(LocationReport {
        location: p.location,
        date: p.date,
        tz_offset_hours: p.tz_offset_hours,
        ephemeris: eph.name().to_owned(),
        day,
        moon_age_at_noon_hours: (noon.jd_ut - conj.jd_ut) * 24.0,
        illumination_at_noon: illum.fraction,
        evening,
        morning,
        map_date_note: crate::dateline::report_note(date, p.location.lon_deg, p.tz_offset_hours),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::{CivilDate, Location};
    use crate::ephemeris::meeus;

    const HMNAO: &str =
        include_str!("../../../tests/fixtures/predictions/hmnao_city_visibility.csv");

    fn minutes_between(a: &UtDateTime, b: &str) -> f64 {
        // b is "YYYY-MM-DD HH:MM" UT.
        let (d, t) = b.split_once(' ').unwrap();
        let p: Vec<u32> = d.split('-').map(|x| x.parse().unwrap()).collect();
        let (h, m) = t.split_once(':').unwrap();
        let want = Instant::from_calendar(
            p[0] as i32,
            p[1],
            p[2],
            h.parse().unwrap(),
            m.parse().unwrap(),
            0.0,
        );
        (a.to_instant().jd_ut - want.jd_ut) * 1440.0
    }

    /// Every HMNAO city evening (Feb/Mar 2026), with coordinates from the
    /// embedded atlas and HMNAO's civil offset: the Yallop verdict in the
    /// report matches HMNAO's category (or is one step off within 0.03 of a
    /// q boundary, where the unstated HMNAO coordinates matter), and sunset
    /// and moonset agree within 2 minutes.
    #[test]
    fn hmnao_city_categories() {
        let eph = meeus(false);
        let (mut n, mut exact) = (0, 0);
        let mut misses = Vec::new();
        for line in HMNAO.lines().skip(1) {
            let f: Vec<&str> = line.split(',').collect();
            let d: Vec<u32> = f[0].split('-').map(|x| x.parse().unwrap()).collect();
            let query = match f[1] {
                "Mecca" => "Makkah",
                "Newcastle" => "Newcastle upon Tyne",
                "Lagos" => "Lagos, Nigeria",
                "London" => "London, GB",
                "Birmingham" => "Birmingham, GB",
                "Manchester" => "Manchester, GB",
                "Glasgow" => "Glasgow, GB",
                "York" => "York, GB",
                "Leeds" => "Leeds, GB",
                "Sheffield" => "Sheffield, GB",
                "Cardiff" => "Cardiff, GB",
                "Belfast" => "Belfast, GB",
                "Bradford" => "Bradford, GB",
                "Leicester" => "Leicester, GB",
                other => other,
            };
            let city = mooncalc_atlas::Atlas::builtin()
                .search(query, 1)
                .into_iter()
                .next()
                .unwrap_or_else(|| panic!("{query} not in the atlas"));
            let r = location_report(
                &*eph,
                &LocationReportParams {
                    date: CivilDate {
                        year: d[0] as i32,
                        month: d[1],
                        day: d[2],
                    },
                    tz_offset_hours: f[3].parse().unwrap(),
                    location: Location {
                        lat_deg: city.lat_deg,
                        lon_deg: city.lon_deg,
                        elevation_m: 0.0,
                    },
                    ephemeris_id: None,
                },
            )
            .unwrap();
            let e = &r.evening;
            assert_eq!(e.criteria.len(), criteria::all().len());
            let y = &e.criteria[0];
            assert_eq!(y.criterion_id, "yallop");
            let (zone, q) = y.verdict.as_ref().map_or((5, f64::NEG_INFINITY), |v| {
                (v.zone as usize, v.value.unwrap())
            });
            let want = "ABCDEF".find(f[5]).unwrap();
            n += 1;
            let near_boundary = mooncalc_visibility::criteria::yallop::Q_BOUNDS
                .iter()
                .any(|b| (q - b).abs() < 0.03);
            if zone == want {
                exact += 1;
            } else if !(zone.abs_diff(want) == 1 && near_boundary) {
                misses.push(format!(
                    "{} {}: ours {zone} (q {q:.3}), HMNAO {want}",
                    f[0], f[1]
                ));
            }
            if !f[8].is_empty() {
                let dt = minutes_between(e.sun_event.as_ref().unwrap(), f[8]);
                assert!(
                    dt.abs() <= 2.0,
                    "{} {} sunset off by {dt:.1} min",
                    f[0],
                    f[1]
                );
            }
            if !f[9].is_empty() {
                let dt = minutes_between(e.moon_event.as_ref().unwrap(), f[9]);
                assert!(
                    dt.abs() <= 2.0,
                    "{} {} moonset off by {dt:.1} min",
                    f[0],
                    f[1]
                );
            }
        }
        assert_eq!(n, 192);
        assert!(misses.is_empty(), "{misses:?}");
        assert!(exact >= 180, "{exact}");
    }

    #[test]
    fn mecca_2026_02_18_report() {
        // HMNAO: Mecca, 2026 February 18, easily visible (A), sunset 15:20
        // UT, moonset 16:19 UT; new Moon 2026-02-17 12:01 UT.
        let eph = meeus(false);
        let r = location_report(
            &*eph,
            &LocationReportParams {
                date: CivilDate {
                    year: 2026,
                    month: 2,
                    day: 18,
                },
                tz_offset_hours: 3.0,
                location: Location {
                    lat_deg: 21.4225,
                    lon_deg: 39.8262,
                    elevation_m: 277.0,
                },
                ephemeris_id: None,
            },
        )
        .unwrap();
        let e = &r.evening;
        assert_eq!(e.status, "crescent");
        let conj = e.conjunction.unwrap();
        assert_eq!((conj.day, conj.hour), (17, 12));
        let g = e.geometry.unwrap();
        assert!((20.0..35.0).contains(&g.age_hours), "{}", g.age_hours);
        assert!((50.0..70.0).contains(&g.lag_minutes.unwrap()));
        let y = e.criteria[0].verdict.as_ref().unwrap();
        assert_eq!(y.code, "A");
        // The geometry is at the best time.
        assert!(!e.geometry_at_sun_event);
        assert_eq!(e.geometry_at, e.best_time);
        // Every criterion is present and the evening day events exist.
        assert!(
            e.criteria
                .iter()
                .all(|c| c.status == "crescent" || c.verdict.is_none())
        );
        assert!(r.day.sunrise.is_some() && r.day.sunset.is_some());
        assert!(r.day.civil_dusk.is_some());
        // The sunset in the day events is the evening's Sun event.
        let s1 = r.day.sunset.unwrap().to_instant().jd_ut;
        let s2 = e.sun_event.unwrap().to_instant().jd_ut;
        assert!((s1 - s2).abs() * 1440.0 < 0.1);
        // Morning of the day after conjunction: no old crescent.
        assert_ne!(r.morning.status, "crescent");
        assert!(r.morning.criteria.iter().all(|c| c.verdict.is_none()));
        assert!(r.moon_age_at_noon_hours > 0.0 && r.illumination_at_noon < 0.05);
    }

    /// Tromsø, 2025-04-01: the young Moon does not set near sunset, so the
    /// best time is undefined; the geometry is the one at sunset (where the
    /// sunset criteria give verdicts), and the conjunction is still given.
    #[test]
    fn moon_always_up_falls_back_to_sunset() {
        let eph = meeus(false);
        let r = location_report(
            &*eph,
            &LocationReportParams {
                date: CivilDate {
                    year: 2025,
                    month: 4,
                    day: 1,
                },
                tz_offset_hours: 2.0,
                location: Location {
                    lat_deg: 69.65,
                    lon_deg: 18.96,
                    elevation_m: 0.0,
                },
                ephemeris_id: None,
            },
        )
        .unwrap();
        let e = &r.evening;
        assert_eq!(e.status, "moon-always-up");
        assert!(e.best_time.is_none() && e.moon_event.is_none());
        assert!(e.geometry_at_sun_event);
        let at = e.geometry_at.unwrap().to_instant().jd_ut;
        assert!((at - e.sun_event.unwrap().to_instant().jd_ut).abs() * 1440.0 < 0.01);
        let g = e.geometry.unwrap();
        assert!(g.lag_minutes.is_none() && g.moon_alt_deg > 20.0, "{g:?}");
        assert_eq!(e.conjunction.unwrap().day, 29);
        let saao = e
            .criteria
            .iter()
            .find(|c| c.criterion_id == "saao")
            .unwrap();
        assert!(saao.verdict.is_some());
        assert_eq!(status_from_code(&e.status), Some(Status::MoonAlwaysUp));
    }

    #[test]
    fn polar_and_invalid_input() {
        let eph = meeus(false);
        let mut p = LocationReportParams {
            date: CivilDate {
                year: 2026,
                month: 6,
                day: 21,
            },
            tz_offset_hours: 0.0,
            location: Location {
                lat_deg: 78.2,
                lon_deg: 15.6,
                elevation_m: 0.0,
            },
            ephemeris_id: None,
        };
        let r = location_report(&*eph, &p).unwrap();
        assert_eq!(r.evening.status, "midnight-sun");
        assert!(r.evening.sun_event.is_none() && r.day.sunset.is_none());
        // No circumstances, but the nearest new Moon (2026-06-15).
        assert!(r.evening.geometry.is_none() && r.evening.geometry_at.is_none());
        assert_eq!(r.evening.conjunction.unwrap().day, 15);
        p.location.lat_deg = 91.0;
        assert!(location_report(&*eph, &p).is_err());
        p.location.lat_deg = 0.0;
        p.tz_offset_hours = 15.0;
        assert!(location_report(&*eph, &p).is_err());
    }
}
