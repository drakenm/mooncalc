//! Civil time zones: IANA zones with their full rules (daylight-saving time
//! and historical changes) or fixed UTC offsets.
//!
//! A [`Zone`] answers "what is the UTC offset at this instant?" and converts
//! local wall-clock time to UT and back, handling the hour skipped when
//! clocks go forward (a *gap*) and the hour repeated when they go back (a
//! *fold*).
//!
//! Data: the IANA Time Zone Database (<https://www.iana.org/time-zones>,
//! public domain), release [`TZDB_VERSION`], compiled to TZif and embedded
//! by the `jiff-tzdb` crate; rules are evaluated by `jiff`. Nothing is read
//! from the operating system, so results are the same on every platform,
//! including `wasm32-unknown-unknown`. To update the data, bump the pinned
//! `jiff-tzdb` (and, if needed, `jiff`) version in the workspace
//! `Cargo.toml` (see `tools/tz/README.md`).
//!
//! Conventions, as everywhere in MoonCalc: instants are Julian Days in UT
//! (UTC is taken as UT; they differ by < 0.9 s), offsets are east-positive,
//! and a *local Julian Day* is the wall-clock reading expressed as a Julian
//! Day (`jd_local = jd_ut + offset / 86400 s`). Before 1582-10-15 dates are
//! Julian-calendar dates; since conversions go through day counts, not
//! calendar fields, the calendar convention does not affect offsets.
//!
//! Before a zone's first rule the tz database gives local mean time of the
//! zone's principal city (e.g. −0:01:15 for London before 1847). Future
//! dates follow the zone's current rule indefinitely. Beyond the year 9000
//! instants are moved back by whole 400-year Gregorian cycles (which repeat
//! weekdays exactly, and with them every recurring DST rule).

use std::fmt;
use std::sync::Mutex;

use jiff::Timestamp;
use jiff::tz::{AmbiguousOffset, Offset, TimeZone};
use mooncalc_core::Instant;
use mooncalc_core::events::CalendarDate;

/// The release of the embedded IANA Time Zone Database, e.g. `"2026e"`.
pub const TZDB_VERSION: &str = match jiff_tzdb::VERSION {
    Some(v) => v,
    None => "unknown",
};

/// Credit line for the time-zone data (public domain; credit is courtesy).
pub const TZ_ATTRIBUTION: &str = "Time-zone rules from the IANA Time Zone Database \
(https://www.iana.org/time-zones), public domain.";

/// Julian Day of the Unix epoch, 1970-01-01 00:00 UT.
const UNIX_EPOCH_JD: f64 = 2_440_587.5;
const SECONDS_PER_DAY: f64 = 86_400.0;
/// 400 Gregorian years, in seconds (146 097 days).
const GREGORIAN_CYCLE_S: i64 = 146_097 * 86_400;
/// Instants after this (about the year 9000) are folded back by 400-year
/// cycles before lookup; jiff's range ends at 9999-12-31.
const FOLD_ABOVE_S: i64 = 221_845_392_000;

/// A civil time zone: an IANA zone, or a fixed offset from UT.
///
/// `Zone` is `Copy` (IANA zones are parsed once and interned for the life of
/// the program), so it can be passed and stored like the plain offset it
/// replaces. Equality compares the zone, not the offset at some instant.
#[derive(Clone, Copy)]
pub struct Zone(Kind);

#[derive(Clone, Copy)]
enum Kind {
    /// Seconds east of Greenwich.
    Fixed(i32),
    Iana(&'static Interned),
}

struct Interned {
    /// The canonical spelling, e.g. `America/New_York`.
    name: &'static str,
    tz: TimeZone,
}

/// Parsed IANA zones, never freed (one entry per distinct zone used).
static INTERNED: Mutex<Vec<&'static Interned>> = Mutex::new(Vec::new());

/// The name is not an IANA zone in the embedded database.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownZone(pub String);

impl fmt::Display for UnknownZone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown time zone {:?}", self.0)
    }
}

impl std::error::Error for UnknownZone {}

/// How a local wall-clock time maps to UT. The values are UT Julian Days.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LocalTime {
    /// The usual case: exactly one instant.
    Unique(f64),
    /// The time was skipped (clocks went forward). `earlier` reads the time
    /// with the offset in force after the change, `later` with the offset
    /// before it; neither instant shows this time on the clock.
    Gap { earlier: f64, later: f64 },
    /// The time occurred twice (clocks went back): first with the offset
    /// before the change (`earlier`), then with the offset after it.
    Fold { earlier: f64, later: f64 },
}

impl LocalTime {
    /// The instant chosen by the usual "compatible" rule (RFC 5545, ECMAScript
    /// Temporal): the earlier of a repeated time, and for a skipped time the
    /// instant reached by reading it with the offset before the gap, i.e. the
    /// clock time moved forward by the length of the gap.
    pub fn compatible(self) -> f64 {
        match self {
            LocalTime::Unique(t) => t,
            LocalTime::Gap { later, .. } => later,
            LocalTime::Fold { earlier, .. } => earlier,
        }
    }

    /// The earlier candidate (the only one when unique).
    pub fn earlier(self) -> f64 {
        match self {
            LocalTime::Unique(t) => t,
            LocalTime::Gap { earlier, .. } | LocalTime::Fold { earlier, .. } => earlier,
        }
    }

    /// The later candidate (the only one when unique).
    pub fn later(self) -> f64 {
        match self {
            LocalTime::Unique(t) => t,
            LocalTime::Gap { later, .. } | LocalTime::Fold { later, .. } => later,
        }
    }
}

/// A change of a zone's UTC offset.
#[derive(Clone, Debug, PartialEq)]
pub struct Transition {
    /// The instant of the change, seconds since 1970-01-01 00:00 UT.
    pub unix_seconds: i64,
    /// The offset from this instant on, seconds east.
    pub offset_seconds: i32,
    /// The abbreviation from this instant on, e.g. `BST`, `-03`.
    pub abbreviation: String,
    /// Whether the new offset is daylight-saving time.
    pub dst: bool,
}

impl Transition {
    /// The instant of the change as a UT Julian Day.
    pub fn jd_ut(&self) -> f64 {
        UNIX_EPOCH_JD + self.unix_seconds as f64 / SECONDS_PER_DAY
    }
}

/// Whole Unix seconds of a Julian Day. Rounded to the millisecond first: a
/// day fraction holds 01:00:00 as 00:59:59.99999…, which must not floor to
/// the second before a transition.
fn unix_seconds(jd: f64) -> f64 {
    (((jd - UNIX_EPOCH_JD) * SECONDS_PER_DAY * 1000.0).round() / 1000.0).floor()
}

/// Unix seconds of a UT Julian Day, folded into jiff's range, and the shift
/// applied (to undo when converting back).
fn to_timestamp(jd: f64) -> (Timestamp, i64) {
    let secs = unix_seconds(jd);
    let mut s = if secs.is_finite() {
        secs.clamp(i64::MIN as f64 / 2.0, i64::MAX as f64 / 2.0) as i64
    } else {
        0
    };
    let mut shift = 0;
    if s > FOLD_ABOVE_S {
        let cycles = (s - FOLD_ABOVE_S).div_euclid(GREGORIAN_CYCLE_S) + 1;
        shift = cycles * GREGORIAN_CYCLE_S;
        s -= shift;
    }
    let min = Timestamp::MIN.as_second() + 2 * 86_400;
    let ts = Timestamp::from_second(s.max(min)).unwrap_or(Timestamp::UNIX_EPOCH);
    (ts, shift)
}

impl Zone {
    /// Universal Time (offset 0).
    pub const UTC: Zone = Zone(Kind::Fixed(0));

    /// A fixed offset, hours east of Greenwich (rounded to the second).
    pub fn fixed_hours(hours: f64) -> Zone {
        let s = if hours.is_finite() {
            (hours * 3600.0).round().clamp(-86_399.0, 86_399.0) as i32
        } else {
            0
        };
        Zone(Kind::Fixed(s))
    }

    /// A fixed offset, seconds east of Greenwich.
    pub fn fixed_seconds(seconds: i32) -> Zone {
        Zone(Kind::Fixed(seconds.clamp(-86_399, 86_399)))
    }

    /// An IANA zone by name, e.g. `Europe/London`. Aliases (`US/Eastern`)
    /// work; the match ignores ASCII case.
    pub fn iana(name: &str) -> Result<Zone, UnknownZone> {
        let name = name.trim();
        let mut zones = INTERNED.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(z) = zones.iter().find(|z| z.name.eq_ignore_ascii_case(name)) {
            return Ok(Zone(Kind::Iana(z)));
        }
        let tz = jiff::tz::db()
            .get(name)
            .map_err(|_| UnknownZone(name.to_owned()))?;
        let canonical = tz.iana_name().unwrap_or(name).to_owned();
        // An alias may resolve to a zone interned under its canonical name.
        if let Some(z) = zones.iter().find(|z| z.name == canonical) {
            return Ok(Zone(Kind::Iana(z)));
        }
        let interned: &'static Interned = Box::leak(Box::new(Interned {
            name: Box::leak(canonical.into_boxed_str()),
            tz,
        }));
        zones.push(interned);
        Ok(Zone(Kind::Iana(interned)))
    }

    /// The IANA zone `name` when given and known, else the fixed offset.
    /// Settings use this: an unknown saved name falls back to the offset.
    pub fn iana_or_fixed(name: Option<&str>, fixed_hours: f64) -> Zone {
        name.filter(|n| !n.trim().is_empty())
            .and_then(|n| Zone::iana(n).ok())
            .unwrap_or_else(|| Zone::fixed_hours(fixed_hours))
    }

    /// The IANA name, for an IANA zone.
    pub fn iana_name(self) -> Option<&'static str> {
        match self.0 {
            Kind::Iana(z) => Some(z.name),
            Kind::Fixed(_) => None,
        }
    }

    /// The offset in hours, for a fixed zone.
    pub fn fixed_offset_hours(self) -> Option<f64> {
        match self.0 {
            Kind::Fixed(s) => Some(s as f64 / 3600.0),
            Kind::Iana(_) => None,
        }
    }

    /// Whether this is a fixed offset (not an IANA zone).
    pub fn is_fixed(self) -> bool {
        matches!(self.0, Kind::Fixed(_))
    }

    /// The UTC offset at UT Julian Day `jd_ut`, seconds east.
    pub fn offset_seconds(self, jd_ut: f64) -> i32 {
        match self.0 {
            Kind::Fixed(s) => s,
            Kind::Iana(z) => z.tz.to_offset(to_timestamp(jd_ut).0).seconds(),
        }
    }

    /// The UTC offset at UT Julian Day `jd_ut`, hours east.
    pub fn offset_hours(self, jd_ut: f64) -> f64 {
        self.offset_seconds(jd_ut) as f64 / 3600.0
    }

    /// The UTC offset at UT Julian Day `jd_ut`, days.
    pub fn offset_days(self, jd_ut: f64) -> f64 {
        self.offset_seconds(jd_ut) as f64 / SECONDS_PER_DAY
    }

    /// The local Julian Day (wall-clock reading) of UT Julian Day `jd_ut`.
    pub fn to_local(self, jd_ut: f64) -> f64 {
        jd_ut + self.offset_days(jd_ut)
    }

    /// The local calendar date of an instant.
    pub fn local_date(self, jd_ut: f64) -> CalendarDate {
        CalendarDate::from_jd(self.to_local(jd_ut))
    }

    /// Whether daylight-saving time is in force at `jd_ut`.
    pub fn is_dst(self, jd_ut: f64) -> bool {
        match self.0 {
            Kind::Fixed(_) => false,
            Kind::Iana(z) => z.tz.to_offset_info(to_timestamp(jd_ut).0).dst().is_dst(),
        }
    }

    /// The zone abbreviation at `jd_ut` (`GMT`, `BST`, `EDT`, `+03`); for a
    /// fixed offset, `UTC` or `UTC±hh:mm`.
    pub fn abbreviation(self, jd_ut: f64) -> String {
        match self.0 {
            Kind::Fixed(s) => format_offset(s),
            Kind::Iana(z) => {
                z.tz.to_offset_info(to_timestamp(jd_ut).0)
                    .abbreviation()
                    .to_owned()
            }
        }
    }

    /// The UT instant(s) at which the clock shows local Julian Day
    /// `jd_local`.
    pub fn resolve_local(self, jd_local: f64) -> LocalTime {
        let z = match self.0 {
            Kind::Fixed(s) => return LocalTime::Unique(jd_local - s as f64 / SECONDS_PER_DAY),
            Kind::Iana(z) => z,
        };
        let (ts, _) = to_timestamp(jd_local);
        let civil = Offset::UTC.to_datetime(ts);
        let at = |o: Offset| jd_local - o.seconds() as f64 / SECONDS_PER_DAY;
        match z.tz.to_ambiguous_timestamp(civil).offset() {
            AmbiguousOffset::Unambiguous { offset } => LocalTime::Unique(at(offset)),
            AmbiguousOffset::Gap { before, after } => LocalTime::Gap {
                earlier: at(after),
                later: at(before),
            },
            AmbiguousOffset::Fold { before, after } => LocalTime::Fold {
                earlier: at(before),
                later: at(after),
            },
        }
    }

    /// UT Julian Day of local Julian Day `jd_local`, by the "compatible"
    /// rule ([`LocalTime::compatible`]).
    pub fn to_ut(self, jd_local: f64) -> f64 {
        self.resolve_local(jd_local).compatible()
    }

    /// UT bounds `[start, end)` of local calendar date `date`: from its first
    /// instant to the first instant of the next date. A day across a DST
    /// change lasts 23 or 25 hours; where clocks skip midnight the day starts
    /// at the first time shown (e.g. 01:00).
    pub fn day_bounds(self, date: CalendarDate) -> (Instant, Instant) {
        let jd0 = date.jd0();
        (
            Instant::from_jd_ut(self.to_ut(jd0)),
            Instant::from_jd_ut(self.to_ut(jd0 + 1.0)),
        )
    }

    /// UT of local civil noon on `date`.
    pub fn civil_noon(self, date: CalendarDate) -> Instant {
        Instant::from_jd_ut(self.to_ut(date.jd0() + 0.5))
    }

    /// UT bounds `[start, end)` of a local calendar year.
    pub fn year_bounds(self, year: i32) -> (Instant, Instant) {
        (
            self.day_bounds(CalendarDate::new(year, 1, 1)).0,
            self.day_bounds(CalendarDate::new(year + 1, 1, 1)).0,
        )
    }

    /// The offset changes in `[from_jd_ut, to_jd_ut)`, in time order. Empty
    /// for a fixed zone. Not folded: the span should lie within the years
    /// −9999..9999.
    pub fn transitions(self, from_jd_ut: f64, to_jd_ut: f64) -> Vec<Transition> {
        let z = match self.0 {
            Kind::Fixed(_) => return Vec::new(),
            Kind::Iana(z) => z,
        };
        let (from, to) = (
            unix_seconds(from_jd_ut) as i64,
            unix_seconds(to_jd_ut) as i64,
        );
        let start = Timestamp::from_second(from - 1).unwrap_or(Timestamp::MIN);
        z.tz.following(start)
            .take_while(|t| t.timestamp().as_second() < to)
            .filter(|t| t.timestamp().as_second() >= from)
            .map(|t| Transition {
                unix_seconds: t.timestamp().as_second(),
                offset_seconds: t.offset().seconds(),
                abbreviation: t.abbreviation().to_owned(),
                dst: t.dst().is_dst(),
            })
            .collect()
    }
}

impl PartialEq for Zone {
    fn eq(&self, other: &Self) -> bool {
        match (self.0, other.0) {
            (Kind::Fixed(a), Kind::Fixed(b)) => a == b,
            (Kind::Iana(a), Kind::Iana(b)) => std::ptr::eq(a, b),
            _ => false,
        }
    }
}

impl Eq for Zone {}

impl std::hash::Hash for Zone {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self.0 {
            Kind::Fixed(s) => s.hash(state),
            Kind::Iana(z) => z.name.hash(state),
        }
    }
}

impl Default for Zone {
    fn default() -> Self {
        Zone::UTC
    }
}

impl fmt::Debug for Zone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Zone({self})")
    }
}

impl fmt::Display for Zone {
    /// The IANA name, or `UTC±hh:mm` for a fixed offset.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Kind::Fixed(s) => f.write_str(&format_offset(s)),
            Kind::Iana(z) => f.write_str(z.name),
        }
    }
}

/// `UTC`, `UTC+05:30`, `UTC−03:00` (seconds shown only when not zero).
pub fn format_offset(seconds: i32) -> String {
    if seconds == 0 {
        return "UTC".into();
    }
    let sign = if seconds > 0 { '+' } else { '−' };
    let a = seconds.unsigned_abs();
    let (h, m, s) = (a / 3600, a / 60 % 60, a % 60);
    if s == 0 {
        format!("UTC{sign}{h:02}:{m:02}")
    } else {
        format!("UTC{sign}{h:02}:{m:02}:{s:02}")
    }
}

/// Every IANA zone name in the embedded database, sorted (about 600,
/// including aliases such as `US/Eastern`).
pub fn zone_names() -> Vec<String> {
    let mut names: Vec<String> = jiff::tz::db()
        .available()
        .map(|n| n.as_str().to_owned())
        .collect();
    names.sort_unstable();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    /// UT Julian Day of a UT calendar time.
    fn jd(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> f64 {
        Instant::from_calendar(y, mo, d, h, mi, 0.0).jd_ut
    }

    fn zone(name: &str) -> Zone {
        Zone::iana(name).unwrap()
    }

    /// Minutes between two Julian Days.
    fn minutes(a: f64, b: f64) -> f64 {
        ((a - b) * 1440.0 * 1000.0).round() / 1000.0
    }

    #[test]
    fn database_is_embedded() {
        assert!(TZDB_VERSION.starts_with("20"), "{TZDB_VERSION}");
        let names = zone_names();
        assert!(names.len() > 300, "{}", names.len());
        assert!(names.iter().any(|n| n == "Asia/Riyadh"));
    }

    #[test]
    fn unknown_zones_are_rejected() {
        assert_eq!(
            Zone::iana("Mars/Olympus_Mons"),
            Err(UnknownZone("Mars/Olympus_Mons".into()))
        );
        assert_eq!(
            Zone::iana_or_fixed(Some("nope"), 2.0),
            Zone::fixed_hours(2.0)
        );
        assert_eq!(Zone::iana_or_fixed(None, 2.0), Zone::fixed_hours(2.0));
        assert_eq!(Zone::iana_or_fixed(Some(""), 2.0), Zone::fixed_hours(2.0));
    }

    #[test]
    fn interning_and_aliases() {
        let a = zone("America/New_York");
        let b = zone("america/new_york");
        assert_eq!(a, b);
        assert_eq!(a.iana_name(), Some("America/New_York"));
        // A link resolves to its target zone.
        let alias = zone("US/Eastern");
        assert_eq!(alias.offset_hours(jd(2024, 7, 1, 12, 0)), -4.0);
        assert_ne!(a, Zone::fixed_hours(-5.0));
        assert_eq!(Zone::fixed_hours(5.5), Zone::fixed_seconds(19_800));
        assert_eq!(a.to_string(), "America/New_York");
        assert_eq!(Zone::fixed_hours(-3.0).to_string(), "UTC−03:00");
    }

    #[test]
    fn london_across_the_2024_bst_change() {
        // Clocks went from 01:00 GMT to 02:00 BST at 01:00 UT on 2024-03-31
        // (EU rule: last Sunday of March, 01:00 UT).
        let z = zone("Europe/London");
        assert_eq!(z.offset_seconds(jd(2024, 3, 31, 0, 59)), 0);
        assert_eq!(z.offset_seconds(jd(2024, 3, 31, 1, 0)), 3600);
        assert_eq!(z.abbreviation(jd(2024, 3, 30, 12, 0)), "GMT");
        assert_eq!(z.abbreviation(jd(2024, 4, 1, 12, 0)), "BST");
        assert!(z.is_dst(jd(2024, 4, 1, 12, 0)));
        // 31 March lasts 23 hours.
        let (s, e) = z.day_bounds(CalendarDate::new(2024, 3, 31));
        assert_eq!(s.jd_ut, jd(2024, 3, 31, 0, 0));
        assert_eq!(minutes(e.jd_ut, s.jd_ut), 23.0 * 60.0);
        // ... and 27 October 25 hours.
        let (s, e) = z.day_bounds(CalendarDate::new(2024, 10, 27));
        assert_eq!(minutes(e.jd_ut, s.jd_ut), 25.0 * 60.0);
        // 01:30 local on 31 March never happened; on 27 October it happened
        // twice, first as BST (00:30 UT), then as GMT (01:30 UT).
        let gap = z.resolve_local(jd(2024, 3, 31, 1, 30));
        assert_eq!(
            gap,
            LocalTime::Gap {
                earlier: jd(2024, 3, 31, 0, 30),
                later: jd(2024, 3, 31, 1, 30)
            }
        );
        assert_eq!(z.to_ut(jd(2024, 3, 31, 1, 30)), jd(2024, 3, 31, 1, 30));
        let fold = z.resolve_local(jd(2024, 10, 27, 1, 30));
        assert_eq!(
            fold,
            LocalTime::Fold {
                earlier: jd(2024, 10, 27, 0, 30),
                later: jd(2024, 10, 27, 1, 30)
            }
        );
        assert_eq!(z.to_ut(jd(2024, 10, 27, 1, 30)), jd(2024, 10, 27, 0, 30));
    }

    #[test]
    fn new_york_across_2024_03_10() {
        // 02:00 EST → 03:00 EDT at 07:00 UT.
        let z = zone("America/New_York");
        assert_eq!(z.offset_hours(jd(2024, 3, 10, 6, 59)), -5.0);
        assert_eq!(z.offset_hours(jd(2024, 3, 10, 7, 0)), -4.0);
        assert_eq!(z.abbreviation(jd(2024, 3, 9, 12, 0)), "EST");
        assert_eq!(z.abbreviation(jd(2024, 3, 11, 12, 0)), "EDT");
        let noon_before = z.civil_noon(CalendarDate::new(2024, 3, 9));
        let noon_after = z.civil_noon(CalendarDate::new(2024, 3, 11));
        assert_eq!(noon_before.jd_ut, jd(2024, 3, 9, 17, 0));
        assert_eq!(noon_after.jd_ut, jd(2024, 3, 11, 16, 0));
    }

    #[test]
    fn pre_2007_us_rules() {
        // Before the Energy Policy Act of 2005 took effect (2007), US DST
        // began on the first Sunday of April: 2006-04-02, and ended on the
        // last Sunday of October (2006-10-29). Under today's rule 2006-03-12
        // would have been EDT.
        let z = zone("America/New_York");
        assert_eq!(z.offset_hours(jd(2006, 3, 20, 12, 0)), -5.0);
        assert_eq!(z.offset_hours(jd(2006, 4, 2, 6, 59)), -5.0);
        assert_eq!(z.offset_hours(jd(2006, 4, 2, 7, 0)), -4.0);
        assert_eq!(z.offset_hours(jd(2006, 10, 29, 5, 59)), -4.0);
        assert_eq!(z.offset_hours(jd(2006, 10, 29, 6, 0)), -5.0);
        assert_eq!(z.offset_hours(jd(2006, 11, 3, 12, 0)), -5.0);
        // Year-round war time, 1943.
        assert_eq!(z.abbreviation(jd(1943, 1, 15, 12, 0)), "EWT");
        assert_eq!(z.offset_hours(jd(1943, 1, 15, 12, 0)), -4.0);
    }

    #[test]
    fn historical_and_local_mean_time() {
        // London kept local mean time (−0:01:15) until 1847-12-01.
        let z = zone("Europe/London");
        assert_eq!(z.offset_seconds(jd(1800, 6, 1, 12, 0)), -75);
        assert_eq!(z.offset_seconds(jd(1900, 6, 1, 12, 0)), 0);
        // Double summer time, 1941.
        assert_eq!(z.offset_hours(jd(1941, 6, 1, 12, 0)), 2.0);
        // Very early and very late instants still give an answer.
        assert_eq!(z.offset_seconds(jd(-4000, 1, 1, 0, 0)), -75);
        let far = jd(12_024, 7, 1, 12, 0);
        assert_eq!(z.offset_hours(far), 1.0);
        assert_eq!(z.offset_hours(jd(12_024, 1, 1, 12, 0)), 0.0);
    }

    #[test]
    fn riyadh_has_no_dst() {
        let z = zone("Asia/Riyadh");
        for (y, m) in [(1990, 1), (2024, 3), (2024, 7), (2030, 10)] {
            assert_eq!(z.offset_hours(jd(y, m, 15, 12, 0)), 3.0);
            assert!(!z.is_dst(jd(y, m, 15, 12, 0)));
        }
        assert!(
            z.transitions(jd(1960, 1, 1, 0, 0), jd(2100, 1, 1, 0, 0))
                .is_empty()
        );
        assert_eq!(z.abbreviation(jd(2024, 3, 15, 0, 0)), "+03");
    }

    #[test]
    fn kolkata_half_hour_zone() {
        let z = zone("Asia/Kolkata");
        let t = jd(2024, 3, 10, 12, 0);
        assert_eq!(z.offset_seconds(t), 19_800);
        assert_eq!(z.abbreviation(t), "IST");
        // Local 2024-03-10 00:00 is 18:30 UT the day before.
        let (s, _) = z.day_bounds(CalendarDate::new(2024, 3, 10));
        assert_eq!(s.jd_ut, jd(2024, 3, 9, 18, 30));
        assert_eq!(
            z.local_date(jd(2024, 3, 9, 18, 30)),
            CalendarDate::new(2024, 3, 10)
        );
        assert_eq!(
            z.local_date(jd(2024, 3, 9, 18, 29)),
            CalendarDate::new(2024, 3, 9)
        );
    }

    #[test]
    fn local_round_trips_around_changes() {
        // Every minute across the London and New York changes of 2024 and
        // a Lord Howe (30-minute DST) change: UT → local → UT is exact, and
        // local → UT → local is exact outside the gaps.
        let cases = [
            ("Europe/London", jd(2024, 3, 31, 1, 0)),
            ("Europe/London", jd(2024, 10, 27, 1, 0)),
            ("America/New_York", jd(2024, 3, 10, 7, 0)),
            ("America/New_York", jd(2024, 11, 3, 6, 0)),
            ("Australia/Lord_Howe", jd(2024, 4, 6, 15, 0)),
        ];
        for (name, change) in cases {
            let z = zone(name);
            for m in -120..=120 {
                let t = change + m as f64 / 1440.0;
                let local = z.to_local(t);
                let back = z.resolve_local(local);
                assert!(
                    (back.earlier() - t).abs() < 1e-9 || (back.later() - t).abs() < 1e-9,
                    "{name} {m}: {back:?}"
                );
                match back {
                    LocalTime::Gap { .. } => panic!("{name} {m}: a shown time in a gap"),
                    LocalTime::Unique(u) => assert!((u - t).abs() < 1e-9),
                    LocalTime::Fold { .. } => {}
                }
                // Local → UT → local.
                let ut = z.to_ut(local);
                assert!((z.to_local(ut) - local).abs() < 1e-9, "{name} {m}");
            }
        }
    }

    #[test]
    fn gap_resolution_moves_forward() {
        // 02:30 on 2024-03-10 in New York does not exist; compatible
        // resolution gives 03:30 EDT (07:30 UT).
        let z = zone("America/New_York");
        let local = jd(2024, 3, 10, 2, 30);
        let ut = z.to_ut(local);
        assert_eq!(ut, jd(2024, 3, 10, 7, 30));
        assert_eq!(minutes(z.to_local(ut), local), 60.0);
    }

    #[test]
    fn midnight_gap_day_starts_at_first_shown_time() {
        // Havana: DST began at 00:00 local on 2024-03-10 (clocks to 01:00).
        let z = zone("America/Havana");
        let (s, e) = z.day_bounds(CalendarDate::new(2024, 3, 10));
        assert_eq!(s.jd_ut, jd(2024, 3, 10, 5, 0));
        assert_eq!(minutes(e.jd_ut, s.jd_ut), 23.0 * 60.0);
        assert_eq!(z.local_date(s.jd_ut), CalendarDate::new(2024, 3, 10));
    }

    #[test]
    fn transitions_list() {
        let z = zone("Europe/London");
        let t = z.transitions(jd(2024, 1, 1, 0, 0), jd(2025, 1, 1, 0, 0));
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].jd_ut(), jd(2024, 3, 31, 1, 0));
        assert_eq!((t[0].offset_seconds, t[0].dst), (3600, true));
        assert_eq!(t[0].abbreviation, "BST");
        assert_eq!(t[1].jd_ut(), jd(2024, 10, 27, 1, 0));
        assert_eq!(t[1].offset_seconds, 0);
        // The span is half-open.
        let at = z.transitions(jd(2024, 3, 31, 1, 0), jd(2024, 3, 31, 1, 1));
        assert_eq!(at.len(), 1);
        assert!(
            z.transitions(jd(2024, 3, 31, 0, 0), jd(2024, 3, 31, 1, 0))
                .is_empty()
        );
    }

    #[test]
    fn fixed_zones() {
        let z = Zone::fixed_hours(-9.5);
        assert!(z.is_fixed());
        assert_eq!(z.fixed_offset_hours(), Some(-9.5));
        assert_eq!(z.offset_hours(jd(2024, 3, 10, 0, 0)), -9.5);
        assert_eq!(z.abbreviation(0.0), "UTC−09:30");
        assert_eq!(z.to_ut(z.to_local(2_460_000.25)), 2_460_000.25);
        assert_eq!(Zone::UTC.abbreviation(0.0), "UTC");
        assert_eq!(format_offset(-75), "UTC−00:01:15");
        assert_eq!(Zone::default(), Zone::UTC);
    }

    #[test]
    fn year_bounds_follow_the_zone() {
        let z = zone("Pacific/Auckland");
        let (s, e) = z.year_bounds(2024);
        // 1 January is in NZDT (UTC+13).
        assert_eq!(s.jd_ut, jd(2023, 12, 31, 11, 0));
        assert_eq!(e.jd_ut, jd(2024, 12, 31, 11, 0));
    }

    #[test]
    fn every_atlas_zone_is_known() {
        // The atlas zones come from GeoNames; each must resolve, and the
        // offsets in force during 2026 must be the atlas's standard and DST
        // offsets (GeoNames' 2026 values). GeoNames still lists Namibian
        // winter time (UTC+1), abolished in 2017; tzdb has UTC+2 all year.
        const STALE_IN_GEONAMES: &[&str] = &["Africa/Windhoek"];
        let atlas = crate::Atlas::builtin();
        let mut seen = std::collections::HashSet::new();
        let start = jd(2026, 1, 1, 12, 0);
        let mut mismatches = Vec::new();
        for c in atlas.cities() {
            if !seen.insert(c.timezone) {
                continue;
            }
            let z = Zone::iana(c.timezone).unwrap_or_else(|e| panic!("{e}"));
            let mut offsets: Vec<f64> =
                (0..365).map(|d| z.offset_hours(start + d as f64)).collect();
            offsets.sort_by(f64::total_cmp);
            offsets.dedup();
            let mut atlas_offsets = vec![c.std_offset_hours, c.dst_offset_hours];
            atlas_offsets.sort_by(f64::total_cmp);
            atlas_offsets.dedup();
            if offsets != atlas_offsets && !STALE_IN_GEONAMES.contains(&c.timezone) {
                mismatches.push(format!("{} {offsets:?} {atlas_offsets:?}", c.timezone));
            }
        }
        assert!(mismatches.is_empty(), "{mismatches:#?}");
        assert!(seen.len() > 300);
    }

    #[test]
    fn zone_is_send_and_sync() {
        fn check<T: Send + Sync + Copy + 'static>() {}
        check::<Zone>();
    }
}
