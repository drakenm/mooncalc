//! Time zones of requests, the offsets put on results, and the zone tables
//! frontends use to show arbitrary instants in local time.
//!
//! A request names its zone with an optional IANA `timeZone` (whose rules,
//! from the embedded IANA Time Zone Database, give the offset on every
//! date, DST and historical changes included) and a fixed `tzOffsetHours`
//! used when no zone is given ([`resolve`]). Local days, civil noon and
//! years are then bounded by the zone's own midnights
//! ([`mooncalc_atlas::Zone::day_bounds`]), so a day across a DST change
//! lasts 23 or 25 hours. Every [`UtDateTime`] in such a service's result
//! carries `offset_hours`, the offset in force at that instant.
//!
//! [`time_zone_offsets`] returns a zone's transitions over a span of years,
//! so a frontend can convert any instant (a time slider, a hover readout)
//! without a call per instant.

use mooncalc_atlas::Zone;
use mooncalc_atlas::tz::{TZDB_VERSION, zone_names};
use mooncalc_core::Instant;
use serde::{Deserialize, Serialize};

use crate::dto::{ApiError, UtDateTime};
use crate::validate;

/// The zone of a request: the IANA zone `time_zone` when given (an unknown
/// name is an error), else the fixed offset `tz_offset_hours` (validated to
/// ±14 h).
pub fn resolve(tz_offset_hours: f64, time_zone: Option<&str>) -> Result<Zone, ApiError> {
    match time_zone.map(str::trim).filter(|n| !n.is_empty()) {
        Some(name) => Zone::iana(name).map_err(|_| {
            ApiError::invalid_params(format!("timeZone {name:?} is not an IANA time zone"))
        }),
        None => {
            validate::tz_offset(tz_offset_hours)?;
            Ok(Zone::fixed_hours(tz_offset_hours))
        }
    }
}

/// A [`UtDateTime`] of `t` with the zone's offset at `t`.
pub fn zoned(t: Instant, zone: Zone) -> UtDateTime {
    UtDateTime {
        offset_hours: Some(zone.offset_hours(t.jd_ut)),
        ..UtDateTime::from(t)
    }
}

/// Puts the zone's offset on every [`UtDateTime`] of a result. Services do
/// this themselves; callers that assemble results from parts (the Tauri
/// shell's parallel eclipse list) call it on the whole.
pub trait Localize {
    fn localize(&mut self, zone: Zone);
}

impl Localize for UtDateTime {
    fn localize(&mut self, zone: Zone) {
        self.offset_hours = Some(zone.offset_hours(self.to_instant().jd_ut));
    }
}

impl<T: Localize> Localize for Option<T> {
    fn localize(&mut self, zone: Zone) {
        if let Some(v) = self {
            v.localize(zone);
        }
    }
}

impl<T: Localize> Localize for Vec<T> {
    fn localize(&mut self, zone: Zone) {
        for v in self {
            v.localize(zone);
        }
    }
}

impl<T: Localize, const N: usize> Localize for [T; N] {
    fn localize(&mut self, zone: Zone) {
        for v in self {
            v.localize(zone);
        }
    }
}

/// `impl Localize` for a struct by localizing the listed fields.
macro_rules! localize_fields {
    ($($t:ty: $($f:ident),+;)+) => {
        $(impl $crate::zone::Localize for $t {
            fn localize(&mut self, zone: mooncalc_atlas::Zone) {
                $($crate::zone::Localize::localize(&mut self.$f, zone);)+
            }
        })+
    };
}
pub(crate) use localize_fields;

// ---------------------------------------------------------------------------
// time_zone_offsets, time_zone_names
// ---------------------------------------------------------------------------

/// Default span of [`time_zone_offsets`]: 1800 (before every zone's first
/// standard time) to 2500, so that years from 2100 on, where every zone
/// repeats its current rule, form a full 400-year Gregorian cycle.
pub const DEFAULT_OFFSET_YEARS: (i32, i32) = (1800, 2500);

/// Longest span of [`time_zone_offsets`], years.
pub const MAX_OFFSET_YEARS: i32 = 1000;

/// Input of `time_zone_offsets`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneOffsetsParams {
    /// IANA time zone, e.g. `America/New_York`.
    pub time_zone: String,
    /// First year (from 1 January 0h UT), default 1800.
    #[serde(default)]
    pub from_year: Option<i32>,
    /// End year, exclusive (to 1 January 0h UT), default 2500.
    #[serde(default)]
    pub to_year: Option<i32>,
}

/// A change of a zone's offset.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneTransitionDto {
    /// Seconds since 1970-01-01 00:00 UT.
    pub unix_seconds: i64,
    /// The offset from this instant on, seconds east of Greenwich.
    pub offset_seconds: i32,
    /// The abbreviation from this instant on, e.g. `BST`, `-03`.
    pub abbreviation: String,
    pub dst: bool,
}

/// A zone's offsets over a span of years.
///
/// The offset at instant `t` (Unix seconds) is that of the last transition
/// at or before `t`, or `initial_*` before the first. Before the span every
/// zone keeps its initial offset (local mean time, for most zones in 1800).
/// After it, fold `t` back by whole 400-year cycles (12 622 780 800 s) into
/// the span's last 400 years, where every zone repeats its current rule.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneOffsets {
    /// The canonical IANA name.
    pub time_zone: String,
    /// IANA Time Zone Database release, e.g. `2026e`.
    pub tzdb_version: String,
    pub from_year: i32,
    pub to_year: i32,
    /// Unix seconds of 1 January `from_year` and `to_year`, 0h UT.
    pub from_unix_seconds: i64,
    pub to_unix_seconds: i64,
    /// The offset in force at the start of the span.
    pub initial_offset_seconds: i32,
    pub initial_abbreviation: String,
    pub initial_dst: bool,
    /// Changes within the span, in time order.
    pub transitions: Vec<ZoneTransitionDto>,
}

fn unix_seconds(t: Instant) -> i64 {
    ((t.jd_ut - 2_440_587.5) * 86_400.0).round() as i64
}

/// A zone's transitions over a span of years (default 1800–2500).
pub fn time_zone_offsets(p: &ZoneOffsetsParams) -> Result<ZoneOffsets, ApiError> {
    if p.time_zone.trim().is_empty() {
        return Err(ApiError::invalid_params("timeZone is required"));
    }
    let zone = resolve(0.0, Some(&p.time_zone))?;
    let from = p.from_year.unwrap_or(DEFAULT_OFFSET_YEARS.0);
    let to = p.to_year.unwrap_or(DEFAULT_OFFSET_YEARS.1);
    if !(-9999..=9999).contains(&from) || !(-9999..=10_000).contains(&to) {
        return Err(ApiError::invalid_params(
            "fromYear and toYear must be between -9999 and 10000",
        ));
    }
    if to <= from || to - from > MAX_OFFSET_YEARS {
        return Err(ApiError::invalid_params(format!(
            "toYear must be after fromYear, by at most {MAX_OFFSET_YEARS} years"
        )));
    }
    let start = Instant::from_calendar(from, 1, 1, 0, 0, 0.0);
    let end = Instant::from_calendar(to, 1, 1, 0, 0, 0.0);
    let transitions = zone
        .transitions(start.jd_ut, end.jd_ut)
        .into_iter()
        .map(|t| ZoneTransitionDto {
            unix_seconds: t.unix_seconds,
            offset_seconds: t.offset_seconds,
            abbreviation: t.abbreviation,
            dst: t.dst,
        })
        .collect();
    Ok(ZoneOffsets {
        time_zone: zone.iana_name().unwrap_or_default().to_owned(),
        tzdb_version: TZDB_VERSION.to_owned(),
        from_year: from,
        to_year: to,
        from_unix_seconds: unix_seconds(start),
        to_unix_seconds: unix_seconds(end),
        initial_offset_seconds: zone.offset_seconds(start.jd_ut),
        initial_abbreviation: zone.abbreviation(start.jd_ut),
        initial_dst: zone.is_dst(start.jd_ut),
        transitions,
    })
}

/// Every IANA zone name in the embedded database, sorted.
pub fn time_zone_names() -> Vec<String> {
    zone_names()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_prefers_the_named_zone() {
        assert_eq!(resolve(3.0, None).unwrap(), Zone::fixed_hours(3.0));
        assert_eq!(resolve(3.0, Some(" ")).unwrap(), Zone::fixed_hours(3.0));
        assert_eq!(
            resolve(3.0, Some("Europe/London")).unwrap().iana_name(),
            Some("Europe/London")
        );
        // The fixed offset is only validated when it is used.
        assert!(resolve(20.0, Some("Asia/Riyadh")).is_ok());
        assert_eq!(resolve(20.0, None).unwrap_err().code, "invalid-params");
        let e = resolve(0.0, Some("Nowhere/Town")).unwrap_err();
        assert!(e.message.contains("Nowhere/Town"), "{}", e.message);
    }

    #[test]
    fn zoned_times_carry_their_offset() {
        let z = Zone::iana("Europe/London").unwrap();
        let winter = zoned(Instant::from_calendar(2024, 3, 30, 12, 0, 0.0), z);
        let summer = zoned(Instant::from_calendar(2024, 4, 1, 12, 0, 0.0), z);
        assert_eq!(winter.offset_hours, Some(0.0));
        assert_eq!(summer.offset_hours, Some(1.0));
        let mut t = Some(UtDateTime::from(Instant::from_calendar(
            2024, 7, 1, 0, 0, 0.0,
        )));
        t.localize(z);
        assert_eq!(t.unwrap().offset_hours, Some(1.0));
    }

    #[test]
    fn offsets_table() {
        let t = time_zone_offsets(&ZoneOffsetsParams {
            time_zone: "europe/london".into(),
            from_year: Some(2024),
            to_year: Some(2025),
        })
        .unwrap();
        assert_eq!(t.time_zone, "Europe/London");
        assert_eq!(t.initial_offset_seconds, 0);
        assert_eq!(t.initial_abbreviation, "GMT");
        assert_eq!(t.transitions.len(), 2);
        // 2024-03-31 01:00 UT.
        assert_eq!(t.transitions[0].unix_seconds, 1_711_846_800);
        assert_eq!(t.transitions[0].offset_seconds, 3600);
        assert_eq!(t.from_unix_seconds, 1_704_067_200);

        let full = time_zone_offsets(&ZoneOffsetsParams {
            time_zone: "America/New_York".into(),
            from_year: None,
            to_year: None,
        })
        .unwrap();
        assert_eq!((full.from_year, full.to_year), DEFAULT_OFFSET_YEARS);
        // Local mean time in 1800.
        assert_eq!(full.initial_offset_seconds, -17_762);
        assert!(full.transitions.len() > 1000);
        let v = serde_json::to_value(&full.transitions[0]).unwrap();
        assert!(v.get("unixSeconds").is_some() && v.get("offsetSeconds").is_some());

        for (from, to) in [(2000, 2000), (1000, 2500), (-20_000, 0)] {
            let bad = ZoneOffsetsParams {
                time_zone: "Asia/Riyadh".into(),
                from_year: Some(from),
                to_year: Some(to),
            };
            assert!(time_zone_offsets(&bad).is_err(), "{from}..{to}");
        }
        let unknown = ZoneOffsetsParams {
            time_zone: "Atlantis/Capital".into(),
            from_year: None,
            to_year: None,
        };
        assert!(time_zone_offsets(&unknown).is_err());
    }

    #[test]
    fn names_include_atlas_zones() {
        let names = time_zone_names();
        assert!(names.iter().any(|n| n == "Asia/Kolkata"));
        assert!(names.windows(2).all(|w| w[0] <= w[1]));
    }
}

/// Services with real zones: local days and offsets across DST changes.
/// Sunrise and sunset references are the HMNAO / USNO values for the
/// places (rounded to the minute), which the Meeus backend reproduces to
/// about a minute.
#[cfg(test)]
mod service_tests {
    use crate::dto::{CivilDate, Location, LocationReportParams, UtDateTime};
    use crate::ephemeris::meeus;
    use crate::report::location_report;
    use crate::tables::dto::{RiseSetMonth, RiseSetParams, YearParams};
    use crate::tables::{lunar_phases, rise_set_month};
    use crate::trajectory::{TrajectoryParams, trajectory_day};

    const LONDON: Location = Location {
        lat_deg: 51.5074,
        lon_deg: -0.1278,
        elevation_m: 0.0,
    };
    const NEW_YORK: Location = Location {
        lat_deg: 40.7128,
        lon_deg: -74.0060,
        elevation_m: 0.0,
    };

    /// Local minutes after midnight of a zoned time, and its offset.
    fn local_minutes(t: &UtDateTime) -> (f64, f64) {
        let off = t.offset_hours.expect("zoned");
        let m = (t.hour as f64 * 60.0 + t.minute as f64 + t.second / 60.0 + off * 60.0)
            .rem_euclid(1440.0);
        (m, off)
    }

    fn hm(h: u32, m: u32) -> f64 {
        (h * 60 + m) as f64
    }

    fn month(zone: Option<&str>, loc: Location, year: i32, m: u32) -> RiseSetMonth {
        rise_set_month(
            &*meeus(false),
            &RiseSetParams {
                year,
                month: m,
                tz_offset_hours: 0.0,
                time_zone: zone.map(str::to_owned),
                location: loc,
                ephemeris_id: None,
            },
        )
        .unwrap()
    }

    /// UT minutes between two zoned times.
    fn ut_minutes(a: &UtDateTime, b: &UtDateTime) -> f64 {
        (a.to_instant().jd_ut - b.to_instant().jd_ut) * 1440.0
    }

    /// Checks a month around a DST change on day `change`: events are the
    /// same instants as at the fixed standard offset, carry the standard
    /// offset before the change and the DST offset after it, so the local
    /// clock time of sunrise jumps by the hour.
    fn check_change(zone: &str, loc: Location, year: i32, m: u32, change: usize, std: f64) {
        let zoned = month(Some(zone), loc, year, m);
        let fixed = rise_set_month(
            &*meeus(false),
            &RiseSetParams {
                year,
                month: m,
                tz_offset_hours: std,
                time_zone: None,
                location: loc,
                ephemeris_id: None,
            },
        )
        .unwrap();
        assert_eq!(zoned.time_zone.as_deref(), Some(zone));
        assert_eq!(zoned.tz_offset_hours, std);
        let rise = |mm: &RiseSetMonth, d: usize| mm.days[d - 1].sun.rise.unwrap().time;
        let set = |mm: &RiseSetMonth, d: usize| mm.days[d - 1].sun.set.unwrap().time;
        for d in (change - 1..=change + 1).filter(|&d| d <= zoned.days.len()) {
            // The UT instants do not depend on the zone.
            assert!(ut_minutes(&rise(&zoned, d), &rise(&fixed, d)).abs() < 1e-3);
            assert!(ut_minutes(&set(&zoned, d), &set(&fixed, d)).abs() < 1e-3);
            let want = if d < change { std } else { std + 1.0 };
            assert_eq!(local_minutes(&rise(&zoned, d)).1, want, "day {d}");
            assert_eq!(local_minutes(&set(&zoned, d)).1, want, "day {d}");
            assert_eq!(local_minutes(&rise(&fixed, d)).1, std);
        }
        let (r0, _) = local_minutes(&rise(&zoned, change - 1));
        let (r1, _) = local_minutes(&rise(&zoned, change));
        let drift = ut_minutes(&rise(&zoned, change), &rise(&zoned, change - 1)) - 1440.0;
        assert!((r1 - r0 - (60.0 + drift)).abs() < 1e-3, "{r0} {r1} {drift}");
    }

    #[test]
    fn london_rise_set_across_the_2024_bst_change() {
        // GMT → BST at 01:00 UT on 31 March 2024.
        check_change("Europe/London", LONDON, 2024, 3, 31, 0.0);
        let m = month(Some("Europe/London"), LONDON, 2024, 3);
        assert_eq!(m.days.len(), 31);
        // Sunrise on 11 March, 06:19 UT (HMNAO, as in the Trajectory test).
        let (r, _) = local_minutes(&m.days[10].sun.rise.unwrap().time);
        assert!((r - hm(6, 19)).abs() < 4.0, "{r}");
    }

    #[test]
    fn new_york_rise_set_across_2024_03_10() {
        // EST → EDT at 07:00 UT on 10 March 2024.
        check_change("America/New_York", NEW_YORK, 2024, 3, 10, -5.0);
    }

    #[test]
    fn pre_2007_us_dst_dates() {
        // In 2006 DST began on 2 April: 20 March was still EST (under the
        // 2007 rule it would have been EDT), 3 April already EDT.
        let mar = month(Some("America/New_York"), NEW_YORK, 2006, 3);
        let (_, off) = local_minutes(&mar.days[19].sun.rise.unwrap().time);
        assert_eq!(off, -5.0);
        let apr = month(Some("America/New_York"), NEW_YORK, 2006, 4);
        let (_, off1) = local_minutes(&apr.days[0].sun.set.unwrap().time);
        let (r3, off3) = local_minutes(&apr.days[2].sun.rise.unwrap().time);
        assert_eq!((off1, off3), (-5.0, -4.0));
        // The 2006 change, on 2 April.
        check_change("America/New_York", NEW_YORK, 2006, 4, 2, -5.0);
        assert!(r3 > hm(6, 0) && r3 < hm(7, 30), "{r3}");
    }

    #[test]
    fn report_follows_the_zone() {
        let report = |zone: &str, date: (i32, u32, u32), loc: Location| {
            location_report(
                &*meeus(false),
                &LocationReportParams {
                    date: CivilDate {
                        year: date.0,
                        month: date.1,
                        day: date.2,
                    },
                    tz_offset_hours: 0.0,
                    time_zone: Some(zone.into()),
                    location: loc,
                    ephemeris_id: None,
                },
            )
            .unwrap()
        };
        let before = report("Europe/London", (2024, 3, 30), LONDON);
        let after = report("Europe/London", (2024, 3, 31), LONDON);
        assert_eq!((before.tz_offset_hours, after.tz_offset_hours), (0.0, 1.0));
        assert_eq!(before.zone_abbreviation, "GMT");
        assert_eq!(after.zone_abbreviation, "BST");
        assert_eq!(after.time_zone.as_deref(), Some("Europe/London"));
        let sunset = after.day.sunset.unwrap();
        assert_eq!(sunset.offset_hours, Some(1.0));
        let (local, _) = local_minutes(&sunset);
        let ut = sunset.hour as f64 * 60.0 + sunset.minute as f64 + sunset.second / 60.0;
        assert!((local - ut - 60.0).abs() < 1e-6);
        // Every time in the report carries an offset.
        let v = serde_json::to_value(&after).unwrap();
        assert_eq!(v["day"]["sunrise"]["offsetHours"], 1.0);
        assert_eq!(v["evening"]["sunEvent"]["offsetHours"], 1.0);

        // No DST in Riyadh; sunset at Mecca about 18:35 local on 2024-03-11
        // (15:35 UT, as in the sky-scene test).
        let mecca = Location {
            lat_deg: 21.4225,
            lon_deg: 39.8262,
            elevation_m: 277.0,
        };
        let r = report("Asia/Riyadh", (2024, 3, 11), mecca);
        assert_eq!(r.tz_offset_hours, 3.0);
        let (ss, off) = local_minutes(&r.day.sunset.unwrap());
        assert_eq!(off, 3.0);
        assert!((ss - hm(18, 35)).abs() < 15.0, "{ss}");
        assert!(r.map_date_note.is_none());
    }

    #[test]
    fn kolkata_year_starts_at_local_midnight() {
        // The first new Moon of 2024 (11 January 11:57 UT) at UTC+5:30.
        let y = lunar_phases(
            &*meeus(false),
            &YearParams {
                year: 2024,
                tz_offset_hours: 0.0,
                time_zone: Some("Asia/Kolkata".into()),
                ephemeris_id: None,
            },
        )
        .unwrap();
        let nm = y
            .lunations
            .iter()
            .find_map(|l| l.phases[0].filter(|t| t.year == 2024))
            .unwrap();
        assert_eq!(nm.offset_hours, Some(5.5));
        assert_eq!((nm.month, nm.day), (1, 11));
        let (m, _) = local_minutes(&nm);
        assert!((m - hm(17, 27)).abs() < 2.0, "{m}");
    }

    #[test]
    fn trajectory_day_lasts_23_hours_at_the_change() {
        let d = trajectory_day(
            &*meeus(false),
            &TrajectoryParams {
                date: CivilDate {
                    year: 2024,
                    month: 3,
                    day: 31,
                },
                tz_offset_hours: 0.0,
                time_zone: Some("Europe/London".into()),
                location: LONDON,
                ephemeris_id: None,
            },
        )
        .unwrap();
        let hours = (d.day_end.to_instant().jd_ut - d.day_start.to_instant().jd_ut) * 24.0;
        assert!((hours - 23.0).abs() < 1e-6, "{hours}");
        assert_eq!(d.samples.len(), 23 * 12 + 1);
        assert_eq!(d.day_start.offset_hours, Some(0.0));
        assert_eq!(d.day_end.offset_hours, Some(1.0));
        assert_eq!(d.sun.rise.unwrap().offset_hours, Some(1.0));
    }
}
