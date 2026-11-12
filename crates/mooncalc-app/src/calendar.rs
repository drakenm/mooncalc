//! Hijri calendars: the catalog, month views, a converter and a comparison
//! of month starts across calendars.
//!
//! The calendars are those of [`mooncalc_hijri`] (Umm al-Qura tables and
//! the arithmetical calendars, [`mooncalc_hijri::calendars`]) plus calendars
//! computed with an ephemeris by [`mooncalc_visibility::calendar`]: the
//! rule calendars ([`RuleCalendar`]: the Umm al-Qura rules, conjunction
//! before sunset at Mecca, conjunction before 12:00 UT) and a
//! visibility-based calendar (a criterion at a threshold zone, at a place,
//! at Mecca or anywhere in a region preset).
//!
//! # Computed calendars and threads
//!
//! Computed month starts are expensive (a region calendar evaluates
//! thousands of observers per evening), so they are computed one Hijri year
//! at a time by [`CalendarEngine::compute_year`], which callers run off the
//! main thread, in parallel if they like (the engine is `Sync`). The view
//! operations ([`CalendarEngine::month`], [`CalendarEngine::convert`],
//! [`CalendarEngine::compare`]) never compute: they use the years computed
//! so far and list the ones they still need in `missing`. A caller computes
//! those and asks again ([`CalendarEngine::compute_missing`] does it
//! sequentially). Built-in calendars never have missing years.
//!
//! # Day convention
//!
//! The Islamic day begins at sunset. As in almanacs, every calendar here
//! identifies a Hijri date with the civil day that begins after its first
//! sunset (see [`mooncalc_hijri::HijriCalendar`]). Civil dates are Julian
//! before 15 October 1582 and Gregorian from then on, like
//! [`crate::dto::CivilDate`].

pub mod dto;

pub use dto::*;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex, MutexGuard};

use mooncalc_core::{Ephemeris, GeoLocation};
use mooncalc_hijri::chained::{month_from_index, month_index};
use mooncalc_hijri::civil::{CivilCalendar, CivilDate as HijriCivilDate};
use mooncalc_hijri::tabular::TABULAR_II_CIVIL;
use mooncalc_hijri::{
    CalendarInfo, CalendarKind, DEFAULT_CALENDAR_ID, GridDay, HijriCalendar, HijriDate, HijriMonth,
    MonthStartProvider, ObservedCalendar, Weekday, calendars, civil_month_grid, hijri_month_grid,
    info_by_id,
};
use mooncalc_visibility::Criterion;
use mooncalc_visibility::calendar::{
    DynProvider, MECCA, Observers, RegionPreset, RuleCalendar, VisibilityProvider,
};

use crate::dto::{ApiError, CivilDate};
use crate::ephemeris::{DEFAULT as DEFAULT_EPHEMERIS, check_covers};
use crate::validate;

/// Civil calendar of every date here: Julian before the 1582 reform,
/// Gregorian after.
pub const CIVIL: CivilCalendar = CivilCalendar::JulianGregorian;

/// Id of the visibility-based calendar.
pub const VISIBILITY_ID: &str = "visibility";

/// Hijri years a computed calendar may be asked for.
pub const COMPUTED_YEARS: std::ops::RangeInclusive<i32> = 1..=9999;

/// The day-convention note shown with calendar views.
pub const DAY_CONVENTION: &str = "The Islamic day begins at sunset. As in almanacs and \
official calendars, each Hijri date is shown on the civil day (midnight to midnight) that \
begins after its first sunset: the evening before already belongs to it.";

/// Grid spacing of region calendars when the request gives none, degrees.
pub const DEFAULT_GRID_STEP_DEG: f64 = 5.0;

const OUTSIDE_RANGE: &str = "outside this calendar's range";
const NOT_COMPUTED: &str = "not computed yet";

/// Providers kept at most (all are dropped when this is exceeded).
const MAX_PROVIDERS: usize = 24;
/// Computed calendars whose month starts are kept at most.
const MAX_KNOWN: usize = 64;

// ---------------------------------------------------------------------------
// Resolving a calendar spec
// ---------------------------------------------------------------------------

/// A computed calendar.
#[derive(Clone)]
enum Computed {
    Rule(RuleCalendar),
    Visibility {
        criterion: Arc<dyn Criterion>,
        max_zone: usize,
        observers: Observers,
        place: String,
    },
}

/// A calendar spec, checked.
#[derive(Clone)]
enum Resolved {
    Builtin(&'static CalendarInfo),
    Computed(Computed),
}

impl Computed {
    fn id(&self) -> &'static str {
        match self {
            Computed::Rule(r) => r.id(),
            Computed::Visibility { .. } => VISIBILITY_ID,
        }
    }

    fn name(&self) -> String {
        match self {
            Computed::Rule(r) => r.name().to_owned(),
            Computed::Visibility {
                criterion,
                max_zone,
                place,
                ..
            } => {
                let zone = criterion
                    .zones()
                    .get(*max_zone)
                    .map_or("?", |z| z.code)
                    .to_owned();
                format!("Visibility: {} ≤ {zone}, {place}", criterion.name())
            }
        }
    }

    /// The configuration, without the ephemeris.
    fn key(&self) -> String {
        match self {
            Computed::Rule(r) => r.id().to_owned(),
            Computed::Visibility {
                criterion,
                max_zone,
                observers,
                ..
            } => format!("visibility|{}|{max_zone}|{observers:?}", criterion.id()),
        }
    }

    fn provider(&self, eph: Arc<dyn Ephemeris>) -> DynProvider {
        match self {
            Computed::Rule(r) => r.provider(eph),
            Computed::Visibility {
                criterion,
                max_zone,
                observers,
                ..
            } => Box::new(VisibilityProvider::new(
                eph,
                criterion.clone(),
                *max_zone,
                observers,
            )),
        }
    }
}

fn computed_description(id: &str) -> CalendarDescription {
    match RuleCalendar::from_id(id) {
        Some(r) => CalendarDescription {
            id: r.id().into(),
            name: r.name().into(),
            short_label: rule_label(r).into(),
            description: r.description().into(),
            family: CalendarFamily::Rule,
            computed: true,
            cldr_id: None,
            attribution: None,
            reference: Some(r.reference().into()),
            first_year: None,
            last_year: None,
        },
        None => CalendarDescription {
            id: VISIBILITY_ID.into(),
            name: "Crescent visibility (computed)".into(),
            short_label: "Visibility".into(),
            description: "A month starts the day after the evening of its 29th day if the \
                          crescent is seen that evening (the criterion reaches the threshold \
                          zone or better at the chosen place, or anywhere in the chosen \
                          region); otherwise the month completes 30 days. Computed with the \
                          selected ephemeris."
                .into(),
            family: CalendarFamily::Visibility,
            computed: true,
            cldr_id: None,
            attribution: None,
            reference: Some(
                "Yallop (1997), Odeh (2004) and the other criteria; the 29th-day rule as in \
                 mooncalc_hijri::chained"
                    .into(),
            ),
            first_year: None,
            last_year: None,
        },
    }
}

impl Resolved {
    fn description(&self) -> CalendarDescription {
        match self {
            Resolved::Builtin(i) => builtin_description(i),
            Resolved::Computed(c) => CalendarDescription {
                name: c.name(),
                ..computed_description(c.id())
            },
        }
    }
}

fn rule_label(r: RuleCalendar) -> &'static str {
    match r {
        RuleCalendar::UmmAlQuraRule => "UQ rule",
        RuleCalendar::ConjunctionBeforeSunsetMecca => "Conj<sunset",
        RuleCalendar::ConjunctionBeforeNoonUt => "Conj<12 UT",
    }
}

/// A short column label for a built-in calendar.
fn builtin_label(id: &str) -> &str {
    match id {
        "umm-al-qura" => "Umm al-Qura",
        "umm-al-qura-icu" => "UQ (ICU)",
        "tabular-1-civil" => "I civil",
        "tabular-1-astronomical" => "I astr.",
        "tabular-2-civil" => "II civil",
        "tabular-2-astronomical" => "II astr.",
        "tabular-3-civil" => "III civil",
        "tabular-3-astronomical" => "III astr.",
        "tabular-4-civil" => "IV civil",
        "tabular-4-astronomical" => "IV astr.",
        other => other,
    }
}

fn builtin_description(i: &CalendarInfo) -> CalendarDescription {
    let range = i.calendar.supported_range();
    CalendarDescription {
        id: i.id.into(),
        name: i.name.into(),
        short_label: builtin_label(i.id).into(),
        description: i.description.into(),
        family: if i.kind == CalendarKind::UmmAlQura {
            CalendarFamily::UmmAlQura
        } else {
            CalendarFamily::Tabular
        },
        computed: false,
        cldr_id: i.cldr_id.map(Into::into),
        attribution: i.attribution.map(Into::into),
        reference: None,
        first_year: Some(range.first_year),
        last_year: Some(range.last_year),
    }
}

fn resolve_visibility(v: &VisibilityCalendarSpec) -> Result<Computed, ApiError> {
    let criterion: Arc<dyn Criterion> = mooncalc_visibility::criteria::by_id(&v.criterion_id)
        .map(Arc::from)
        .ok_or_else(|| ApiError::unknown_criterion(&v.criterion_id))?;
    let max_zone = usize::from(v.max_zone);
    if max_zone >= criterion.zones().len() {
        return Err(ApiError::invalid_params(format!(
            "{} has no zone {}",
            criterion.name(),
            v.max_zone
        )));
    }
    let (observers, place) = match v.observers.as_str() {
        OBSERVERS_MECCA => (Observers::Place(MECCA), "Mecca".to_owned()),
        OBSERVERS_LOCATION => {
            let l = v.location.ok_or_else(|| {
                ApiError::invalid_params("the visibility calendar needs a location")
            })?;
            validate::location(l)?;
            let name = v
                .location_name
                .clone()
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| {
                    crate::atlas::nearest_city(&crate::dto::NearestCityParams {
                        lat_deg: l.lat_deg,
                        lon_deg: l.lon_deg,
                    })
                    .ok()
                    .flatten()
                    .map_or_else(
                        || format!("{:.2}°, {:.2}°", l.lat_deg, l.lon_deg),
                        |c| c.city.name,
                    )
                });
            (
                Observers::Place(GeoLocation::new(l.lat_deg, l.lon_deg, l.elevation_m)),
                name,
            )
        }
        id => {
            let r = RegionPreset::from_id(id)
                .ok_or_else(|| ApiError::invalid_params(format!("unknown observers {id:?}")))?;
            let step = v.grid_step_deg.unwrap_or(DEFAULT_GRID_STEP_DEG);
            if !step.is_finite() {
                return Err(ApiError::invalid_params("gridStepDeg must be a number"));
            }
            (
                Observers::Region {
                    region: r.region(),
                    step_deg: step.clamp(1.0, 30.0),
                },
                format!("anywhere in {}", r.name()),
            )
        }
    };
    Ok(Computed::Visibility {
        criterion,
        max_zone,
        observers,
        place,
    })
}

fn resolve(spec: &CalendarSpec) -> Result<Resolved, ApiError> {
    if let Some(info) = info_by_id(&spec.id) {
        return Ok(Resolved::Builtin(info));
    }
    if let Some(r) = RuleCalendar::from_id(&spec.id) {
        return Ok(Resolved::Computed(Computed::Rule(r)));
    }
    if spec.id == VISIBILITY_ID {
        let v = spec.visibility.as_ref().ok_or_else(|| {
            ApiError::invalid_params("the visibility calendar needs its settings (visibility)")
        })?;
        return resolve_visibility(v).map(Resolved::Computed);
    }
    Err(ApiError::invalid_params(format!(
        "unknown calendar {:?}",
        spec.id
    )))
}

/// The spec of a computed calendar named in a [`MissingYear`], given the
/// request's visibility settings.
pub fn spec_for(calendar_id: &str, visibility: Option<&VisibilityCalendarSpec>) -> CalendarSpec {
    CalendarSpec {
        id: calendar_id.to_owned(),
        visibility: visibility.cloned(),
    }
}

// ---------------------------------------------------------------------------
// Catalog
// ---------------------------------------------------------------------------

/// Every calendar with the names a view needs.
pub fn calendar_list(params: &CalendarListParams) -> CalendarCatalog {
    let (uaq, tabular): (Vec<_>, Vec<_>) = calendars()
        .iter()
        .partition(|i| i.kind == CalendarKind::UmmAlQura);
    let visibility = params
        .visibility
        .as_ref()
        .and_then(|v| resolve_visibility(v).ok())
        .map_or_else(
            || computed_description(VISIBILITY_ID),
            |c| CalendarDescription {
                name: c.name(),
                ..computed_description(VISIBILITY_ID)
            },
        );
    let calendars = uaq
        .into_iter()
        .map(builtin_description)
        .chain(std::iter::once(visibility))
        .chain(
            RuleCalendar::ALL
                .into_iter()
                .map(|r| computed_description(r.id())),
        )
        .chain(tabular.into_iter().map(builtin_description))
        .collect();
    CalendarCatalog {
        calendars,
        default_id: DEFAULT_CALENDAR_ID.into(),
        regions: RegionPreset::ALL
            .into_iter()
            .map(|r| RegionDescription {
                id: r.id().into(),
                name: r.name().into(),
            })
            .collect(),
        hijri_months: HijriMonth::ALL
            .into_iter()
            .map(|m| HijriMonthName {
                number: m.number(),
                name: m.name().into(),
                ascii: m.name_ascii().into(),
                arabic: m.name_arabic().into(),
            })
            .collect(),
        weekdays: Weekday::ALL
            .into_iter()
            .map(|w| WeekdayName {
                index: w.index_from_sunday(),
                english: w.name_english().into(),
                transliterated: w.name_transliterated().into(),
                arabic: w.name_arabic().into(),
            })
            .collect(),
        day_convention: DAY_CONVENTION.into(),
    }
}

// ---------------------------------------------------------------------------
// Conversions between DTOs and calendar types
// ---------------------------------------------------------------------------

fn civil_to_dto(c: HijriCivilDate) -> CivilDate {
    CivilDate {
        year: c.year,
        month: u32::from(c.month),
        day: u32::from(c.day),
    }
}

/// The JDN of a civil date, or `None` if there is no such date.
pub fn civil_jdn(d: CivilDate) -> Option<i64> {
    let month = u8::try_from(d.month).ok()?;
    let day = u8::try_from(d.day).ok()?;
    HijriCivilDate::new(d.year, month, day).to_jdn(CIVIL)
}

/// The civil date of a JDN.
pub fn jdn_civil(jdn: i64) -> CivilDate {
    civil_to_dto(HijriCivilDate::from_jdn(jdn, CIVIL))
}

fn hijri_to_dto(h: HijriDate) -> HijriYmd {
    HijriYmd {
        year: h.year,
        month: h.month,
        day: h.day,
    }
}

fn grid_day(d: &GridDay) -> CalendarDay {
    CalendarDay {
        jdn: d.jdn,
        civil: civil_to_dto(d.civil),
        hijri: d.hijri.map(hijri_to_dto),
        weekday: d.weekday.index_from_sunday(),
    }
}

fn day_of(cal: &dyn HijriCalendar, jdn: i64) -> CalendarDay {
    CalendarDay {
        jdn,
        civil: jdn_civil(jdn),
        hijri: cal.to_hijri(jdn).map(hijri_to_dto),
        weekday: Weekday::from_jdn(jdn).index_from_sunday(),
    }
}

fn month_start(year: i32, month: u8, jdn: i64) -> MonthStart {
    MonthStart {
        year,
        month,
        jdn,
        civil: jdn_civil(jdn),
    }
}

/// The Hijri year containing civil day `jdn`, close enough to choose which
/// years to compute (the tabular calendar is within a few days of any lunar
/// calendar).
fn approximate_hijri_year(jdn: i64) -> Option<i32> {
    TABULAR_II_CIVIL.to_hijri(jdn).map(|h| h.year)
}

fn step_month(m: MonthRef, delta: i32) -> MonthRef {
    let index = m.year * 12 + i32::from(m.month) - 1 + delta;
    MonthRef {
        year: index.div_euclid(12),
        month: (index.rem_euclid(12) + 1) as u8,
    }
}

fn weekday_from_index(i: u8) -> Result<Weekday, ApiError> {
    Weekday::ALL
        .get(usize::from(i))
        .copied()
        .ok_or_else(|| ApiError::invalid_params("weekStart must be 0 (Sunday) to 6 (Saturday)"))
}

// ---------------------------------------------------------------------------
// The engine
// ---------------------------------------------------------------------------

/// Month starts known so far, as a provider (`None` for months not
/// computed): month index (`mooncalc_hijri::chained::month_index`) → JDN.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnownStarts(pub BTreeMap<i64, i64>);

impl MonthStartProvider for KnownStarts {
    fn month_start(&self, hijri_year: i32, month: u8) -> Option<i64> {
        if !(1..=12).contains(&month) {
            return None;
        }
        self.0.get(&month_index(hijri_year, month)).copied()
    }
}

/// The computed years of one calendar configuration.
#[derive(Default)]
struct Known {
    /// Month index → JDN.
    starts: BTreeMap<i64, i64>,
    years: BTreeSet<i32>,
}

/// A calendar to read from: a built-in one, or a computed one with the
/// years computed so far ([`CalendarEngine::view_calendar`]).
#[derive(Clone)]
pub enum ViewCalendar {
    Builtin(&'static (dyn HijriCalendar + Sync)),
    Computed(Arc<ObservedCalendar<KnownStarts>>),
}

impl ViewCalendar {
    pub fn get(&self) -> &dyn HijriCalendar {
        match self {
            ViewCalendar::Builtin(c) => *c,
            ViewCalendar::Computed(c) => c.as_ref(),
        }
    }
}

/// Computed month starts and their providers, shared by the requests of a
/// session. `Sync`: share one between threads (e.g. in an `Arc`).
#[derive(Default)]
pub struct CalendarEngine {
    providers: Mutex<HashMap<String, Arc<DynProvider>>>,
    known: Mutex<HashMap<String, Known>>,
}

impl std::fmt::Debug for CalendarEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CalendarEngine")
            .field("providers", &lock(&self.providers).len())
            .field("known", &lock(&self.known).len())
            .finish()
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A poisoned map only means another thread panicked mid-insert; the map
    // itself is still consistent.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Cache key of a computed calendar with an ephemeris.
fn engine_key(c: &Computed, eph_id: Option<&str>, eph: &dyn Ephemeris) -> String {
    format!(
        "{}|{}|{}",
        c.key(),
        eph_id.unwrap_or(DEFAULT_EPHEMERIS),
        eph.name()
    )
}

/// Days before a month start whose ephemeris data the start depends on:
/// the conjunction (up to two days earlier), the evening tests from the day
/// before it, and each evening's searches (±1.75 d around local noon, which
/// is up to half a day from UT noon).
pub const START_MARGIN_BEFORE_DAYS: f64 = 5.0;
/// Days after a month start whose data it depends on (its eve's searches).
pub const START_MARGIN_AFTER_DAYS: f64 = 3.0;
/// Largest difference between the tabular estimate of a month start and
/// a computed one, days.
const TABULAR_ERROR_DAYS: i64 = 3;

/// Whether the ephemeris covers everything the month starting on civil day
/// `jdn` depends on. Computed calendars drop starts too close to (or
/// beyond) the edges of the coverage rather than show them wrong: outside
/// it the evening tests see no crescent, which would silently lengthen
/// months.
pub fn start_covered(eph: &dyn Ephemeris, jdn: i64) -> bool {
    let jd = jdn as f64;
    check_covers(
        eph,
        jd - START_MARGIN_BEFORE_DAYS,
        jd + START_MARGIN_AFTER_DAYS,
    )
    .is_ok()
}

/// Whether a month whose tabular start is `estimate` may start inside the
/// coverage (otherwise it is not computed at all).
fn estimate_covered(eph: &dyn Ephemeris, estimate: i64) -> bool {
    start_covered(eph, estimate - TABULAR_ERROR_DAYS)
        || start_covered(eph, estimate + TABULAR_ERROR_DAYS)
}

/// How much of a Hijri year an ephemeris covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YearCoverage {
    Full,
    /// Some months: a computed calendar gives the covered ones.
    Partial,
    /// None: a computed calendar is not computed for the year.
    Outside,
}

/// The coverage of Hijri `year`, from the tabular estimate of its months.
pub fn hijri_year_coverage(eph: &dyn Ephemeris, year: i32) -> YearCoverage {
    let starts: Vec<i64> = (1..=12)
        .filter_map(|m| TABULAR_II_CIVIL.month_start(year, m))
        .collect();
    let covered = starts
        .iter()
        .filter(|&&s| {
            start_covered(eph, s - TABULAR_ERROR_DAYS) && start_covered(eph, s + TABULAR_ERROR_DAYS)
        })
        .count();
    if !starts.is_empty() && covered == starts.len() {
        YearCoverage::Full
    } else if starts.iter().any(|&s| estimate_covered(eph, s)) {
        YearCoverage::Partial
    } else {
        YearCoverage::Outside
    }
}

impl CalendarEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget every computed month start (e.g. after loading another
    /// ephemeris file with the same id).
    pub fn clear(&self) {
        lock(&self.providers).clear();
        lock(&self.known).clear();
    }

    fn provider(&self, key: &str, c: &Computed, eph: &Arc<dyn Ephemeris>) -> Arc<DynProvider> {
        if let Some(p) = lock(&self.providers).get(key) {
            return p.clone();
        }
        // Built outside the lock: a region calendar's provider samples its
        // grid on construction.
        let p = Arc::new(c.provider(eph.clone()));
        let mut map = lock(&self.providers);
        if map.len() >= MAX_PROVIDERS {
            map.clear();
        }
        map.entry(key.to_owned()).or_insert(p).clone()
    }

    /// Compute (or look up) the month starts of one Hijri year: the
    /// expensive step of a computed calendar, safe to run on several threads
    /// at once. For a built-in calendar it just reads the table.
    ///
    /// A computed calendar gives the months the ephemeris covers
    /// ([`start_covered`]); a year it does not cover at all is an error
    /// ([`hijri_year_coverage`]).
    pub fn compute_year(
        &self,
        eph: &Arc<dyn Ephemeris>,
        params: &MonthStartsParams,
    ) -> Result<MonthStartsYear, ApiError> {
        self.compute_year_with(eph, params, |_, _| true)
    }

    /// As [`Self::compute_year`], calling `progress(done, total)` after each
    /// month computed; returning `false` cancels (a `cancelled` error, and
    /// nothing of the year is kept).
    pub fn compute_year_with(
        &self,
        eph: &Arc<dyn Ephemeris>,
        params: &MonthStartsParams,
        mut progress: impl FnMut(usize, usize) -> bool,
    ) -> Result<MonthStartsYear, ApiError> {
        let resolved = resolve(&params.calendar)?;
        let year = params.hijri_year;
        if !COMPUTED_YEARS.contains(&year) {
            return Err(ApiError::invalid_params(
                "hijriYear must be between 1 and 9999",
            ));
        }
        let first = month_index(year, 1);
        let indices = first..=first + 12;
        let (id, name, starts) = match &resolved {
            Resolved::Builtin(info) => {
                let starts = indices
                    .filter_map(|n| {
                        let (y, m) = month_from_index(n);
                        Some(month_start(y, m, info.calendar.month_start(y, m)?))
                    })
                    .collect();
                (info.id.to_owned(), info.name.to_owned(), starts)
            }
            Resolved::Computed(c) => {
                if hijri_year_coverage(&**eph, year) == YearCoverage::Outside {
                    return Err(ApiError::invalid_params(format!(
                        "{year} AH is outside the range of {}",
                        eph.name()
                    )));
                }
                let key = engine_key(c, params.ephemeris_id.as_deref(), &**eph);
                let found: Vec<(i64, i64)> = {
                    let known = lock(&self.known);
                    match known.get(&key) {
                        Some(k) if k.years.contains(&year) => indices
                            .clone()
                            .filter_map(|n| k.starts.get(&n).map(|&j| (n, j)))
                            .collect(),
                        _ => Vec::new(),
                    }
                };
                let found = if found.is_empty() {
                    let provider = self.provider(&key, c, eph);
                    let total = indices.clone().count();
                    let mut found: Vec<(i64, i64)> = Vec::with_capacity(total);
                    for (i, n) in indices.enumerate() {
                        let (y, m) = month_from_index(n);
                        // Skip months entirely outside the coverage; keep
                        // only starts whose data is all inside it.
                        let estimate = TABULAR_II_CIVIL.month_start(y, m);
                        if estimate.is_none_or(|e| estimate_covered(&**eph, e))
                            && let Some(j) = provider.month_start(y, m)
                            && start_covered(&**eph, j)
                        {
                            found.push((n, j));
                        }
                        if !progress(i + 1, total) {
                            return Err(ApiError::cancelled());
                        }
                    }
                    let mut known = lock(&self.known);
                    if known.len() >= MAX_KNOWN && !known.contains_key(&key) {
                        known.clear();
                    }
                    let k = known.entry(key).or_default();
                    k.starts.extend(found.iter().copied());
                    k.years.insert(year);
                    found
                } else {
                    found
                };
                let starts = found
                    .into_iter()
                    .map(|(n, j)| {
                        let (y, m) = month_from_index(n);
                        month_start(y, m, j)
                    })
                    .collect();
                (c.id().to_owned(), c.name(), starts)
            }
        };
        Ok(MonthStartsYear {
            calendar_id: id,
            calendar_name: name,
            hijri_year: year,
            starts,
        })
    }

    /// Compute every missing year sequentially (the single-threaded way to
    /// complete a view; parallel callers use [`Self::compute_year`]).
    pub fn compute_missing(
        &self,
        eph: &Arc<dyn Ephemeris>,
        ephemeris_id: Option<&str>,
        visibility: Option<&VisibilityCalendarSpec>,
        missing: &[MissingYear],
    ) -> Result<(), ApiError> {
        for m in missing {
            self.compute_year(
                eph,
                &MonthStartsParams {
                    calendar: spec_for(&m.calendar_id, visibility),
                    hijri_year: m.hijri_year,
                    ephemeris_id: ephemeris_id.map(Into::into),
                },
            )?;
        }
        Ok(())
    }

    /// The calendar of `spec` as far as computed with `eph`: a built-in
    /// table, or a computed calendar over the years computed so far (empty
    /// before any). Never computes; frontends that lay out months
    /// themselves read dates from it.
    pub fn view_calendar(
        &self,
        eph: &dyn Ephemeris,
        spec: &CalendarSpec,
        ephemeris_id: Option<&str>,
    ) -> Result<ViewCalendar, ApiError> {
        let resolved = resolve(spec)?;
        Ok(self.calendar(&resolved, ephemeris_id, eph, &[]).0)
    }

    /// Which of the `wanted` Hijri years of `spec` still need
    /// [`Self::compute_year`] (none for a built-in calendar; years the
    /// ephemeris does not cover at all are never listed).
    pub fn missing_years(
        &self,
        eph: &dyn Ephemeris,
        spec: &CalendarSpec,
        ephemeris_id: Option<&str>,
        wanted: &[i32],
    ) -> Result<Vec<i32>, ApiError> {
        let resolved = resolve(spec)?;
        let (_, missing) = self.calendar(&resolved, ephemeris_id, eph, wanted);
        Ok(missing.into_iter().map(|m| m.hijri_year).collect())
    }

    /// The calendar to read and the wanted years it lacks.
    fn calendar(
        &self,
        resolved: &Resolved,
        eph_id: Option<&str>,
        eph: &dyn Ephemeris,
        wanted: &[i32],
    ) -> (ViewCalendar, Vec<MissingYear>) {
        let c = match resolved {
            Resolved::Builtin(info) => return (ViewCalendar::Builtin(info.calendar), Vec::new()),
            Resolved::Computed(c) => c,
        };
        let key = engine_key(c, eph_id, eph);
        let known = lock(&self.known);
        let k = known.get(&key);
        let mut seen = BTreeSet::new();
        let missing = wanted
            .iter()
            .copied()
            .filter(|&y| {
                COMPUTED_YEARS.contains(&y)
                    && seen.insert(y)
                    && !k.is_some_and(|k| k.years.contains(&y))
                    && hijri_year_coverage(eph, y) != YearCoverage::Outside
            })
            .map(|hijri_year| MissingYear {
                calendar_id: c.id().to_owned(),
                hijri_year,
            })
            .collect();
        let (starts, first, last) = match k {
            Some(k) if !k.years.is_empty() => (
                k.starts.clone(),
                *k.years.first().expect("non-empty"),
                *k.years.last().expect("non-empty"),
            ),
            _ => (BTreeMap::new(), 1, 0),
        };
        let cal = ObservedCalendar::new(KnownStarts(starts), c.id(), c.name(), first, last);
        (ViewCalendar::Computed(Arc::new(cal)), missing)
    }

    /// A month of the primary calendar laid out in weeks.
    pub fn month(
        &self,
        eph: &dyn Ephemeris,
        params: &CalendarMonthParams,
    ) -> Result<CalendarMonth, ApiError> {
        let resolved = resolve(&params.calendar)?;
        let week_start = weekday_from_index(params.week_start.unwrap_or(6))?;
        let eph_id = params.ephemeris_id.as_deref();
        let computed = matches!(resolved, Resolved::Computed(_));

        // Which month: given, or the one containing the date.
        let month = match (params.month, params.date) {
            (Some(m), _) => {
                if !(1..=12).contains(&m.month) {
                    return Err(ApiError::invalid_params("month must be between 1 and 12"));
                }
                m
            }
            (None, Some(d)) => {
                validate::date(d)?;
                let jdn = civil_jdn(d).ok_or_else(|| ApiError::invalid_params("no such date"))?;
                match params.primary {
                    PrimaryCalendar::Gregorian => MonthRef {
                        year: d.year,
                        month: d.month as u8,
                    },
                    PrimaryCalendar::Hijri => {
                        // Use the computed years so far; fall back on the
                        // tabular estimate (computed calendars) or the
                        // range's end (built-in ones).
                        let estimate = approximate_hijri_year(jdn).unwrap_or(1);
                        let wanted = [estimate, estimate - 1, estimate + 1];
                        let (cal, _) = self.calendar(&resolved, eph_id, eph, &wanted);
                        let cal = cal.get();
                        match cal.to_hijri(jdn) {
                            Some(h) => MonthRef {
                                year: h.year,
                                month: h.month,
                            },
                            None if computed => {
                                let h = TABULAR_II_CIVIL
                                    .to_hijri(jdn)
                                    .unwrap_or(HijriDate::new(1, 1, 1));
                                MonthRef {
                                    year: h.year,
                                    month: h.month,
                                }
                            }
                            None => {
                                let r = cal.supported_range();
                                if cal.supported_jdn_range().is_some_and(|(a, _)| jdn < a) {
                                    MonthRef {
                                        year: r.first_year,
                                        month: 1,
                                    }
                                } else {
                                    MonthRef {
                                        year: r.last_year,
                                        month: 12,
                                    }
                                }
                            }
                        }
                    }
                }
            }
            (None, None) => {
                return Err(ApiError::invalid_params(
                    "calendar_month needs a month or a date",
                ));
            }
        };

        // The computed years the month needs.
        let wanted: Vec<i32> = match params.primary {
            PrimaryCalendar::Hijri => vec![month.year],
            PrimaryCalendar::Gregorian => {
                let first = civil_jdn(CivilDate {
                    year: month.year,
                    month: u32::from(month.month),
                    day: 1,
                });
                first
                    .map(|j| {
                        [j - 3, j + 34]
                            .into_iter()
                            .filter_map(approximate_hijri_year)
                            .collect()
                    })
                    .unwrap_or_default()
            }
        };
        let (cal, missing) = self.calendar(&resolved, eph_id, eph, &wanted);
        let cal = cal.get();
        let in_years = match params.primary {
            PrimaryCalendar::Hijri => (1..=9999).contains(&month.year),
            PrimaryCalendar::Gregorian => (-4712..=9999).contains(&month.year),
        };
        let grid = in_years
            .then(|| match params.primary {
                PrimaryCalendar::Hijri => hijri_month_grid(cal, month.year, month.month, CIVIL),
                PrimaryCalendar::Gregorian => civil_month_grid(cal, month.year, month.month, CIVIL),
            })
            .flatten();
        let columns = (0..7)
            .map(|i| (week_start.index_from_sunday() + i) % 7)
            .collect();
        let description = resolved.description();
        let (weeks, other_months, message) = match grid {
            Some(g) => {
                let weeks = g
                    .weeks(week_start)
                    .iter()
                    .map(|row| row.iter().map(|c| c.as_ref().map(grid_day)).collect())
                    .collect();
                let other = match params.primary {
                    PrimaryCalendar::Hijri => g.civil_months(),
                    PrimaryCalendar::Gregorian => g.hijri_months(),
                }
                .into_iter()
                .map(|(year, month)| MonthRef { year, month })
                .collect();
                let message = (params.primary == PrimaryCalendar::Gregorian
                    && g.days.iter().any(|d| d.hijri.is_none()))
                .then(|| {
                    format!(
                        "Some days are {}.",
                        if missing.is_empty() {
                            OUTSIDE_RANGE
                        } else {
                            NOT_COMPUTED
                        }
                    )
                });
                (weeks, other, message)
            }
            None => {
                let why = if !missing.is_empty() {
                    NOT_COMPUTED.to_owned()
                } else {
                    match (description.first_year, description.last_year) {
                        (Some(a), Some(b)) => format!("{OUTSIDE_RANGE} ({a}–{b} AH)"),
                        _ => format!("{OUTSIDE_RANGE} or the ephemeris's"),
                    }
                };
                (
                    Vec::new(),
                    Vec::new(),
                    Some(format!("This month is {why}.")),
                )
            }
        };
        Ok(CalendarMonth {
            calendar_id: description.id,
            calendar_name: description.name,
            primary: params.primary,
            month,
            prev: step_month(month, -1),
            next: step_month(month, 1),
            columns,
            weeks,
            other_months,
            message,
            missing,
        })
    }

    /// Convert a civil date to the calendar's Hijri date or back.
    pub fn convert(
        &self,
        eph: &dyn Ephemeris,
        params: &CalendarConvertParams,
    ) -> Result<CalendarConversion, ApiError> {
        let resolved = resolve(&params.calendar)?;
        let eph_id = params.ephemeris_id.as_deref();
        let none = |message: String, missing| CalendarConversion {
            day: None,
            month_length: None,
            message: Some(message),
            missing,
        };
        match (params.civil, params.hijri) {
            (Some(d), None) => {
                let Some(jdn) = (-9999..=9999)
                    .contains(&d.year)
                    .then(|| civil_jdn(d))
                    .flatten()
                else {
                    return Ok(none("No such date.".into(), Vec::new()));
                };
                let wanted: Vec<i32> = [jdn - 3, jdn + 3]
                    .into_iter()
                    .filter_map(approximate_hijri_year)
                    .collect();
                let (cal, missing) = self.calendar(&resolved, eph_id, eph, &wanted);
                let cal = cal.get();
                let day = day_of(cal, jdn);
                let month_length = day.hijri.and_then(|h| cal.month_length(h.year, h.month));
                let message = day.hijri.is_none().then(|| {
                    format!(
                        "This date is {}.",
                        if missing.is_empty() {
                            OUTSIDE_RANGE
                        } else {
                            NOT_COMPUTED
                        }
                    )
                });
                Ok(CalendarConversion {
                    day: Some(day),
                    month_length,
                    message,
                    missing,
                })
            }
            (None, Some(h)) => {
                if !(1..=12).contains(&h.month) || !(1..=30).contains(&h.day) {
                    return Ok(none("No such date.".into(), Vec::new()));
                }
                let (cal, missing) = self.calendar(&resolved, eph_id, eph, &[h.year]);
                let cal = cal.get();
                let month_length = cal.month_length(h.year, h.month);
                match cal.from_hijri(HijriDate::new(h.year, h.month, h.day)) {
                    Some(jdn) => Ok(CalendarConversion {
                        day: Some(day_of(cal, jdn)),
                        month_length,
                        message: None,
                        missing,
                    }),
                    None => {
                        let message = match month_length {
                            Some(n) => format!("No such date: this month has {n} days."),
                            None if !missing.is_empty() => {
                                format!("This year is {NOT_COMPUTED}.")
                            }
                            None => {
                                let d = resolved.description();
                                match (d.first_year, d.last_year) {
                                    (Some(a), Some(b)) => {
                                        format!("Outside this calendar's range ({a}–{b} AH).")
                                    }
                                    _ => "Outside this calendar's range.".into(),
                                }
                            }
                        };
                        Ok(CalendarConversion {
                            day: None,
                            month_length,
                            message: Some(message),
                            missing,
                        })
                    }
                }
            }
            _ => Err(ApiError::invalid_params(
                "give exactly one of civil and hijri",
            )),
        }
    }

    /// The start of every month of a Hijri year under every calendar.
    pub fn compare(
        &self,
        eph: &dyn Ephemeris,
        params: &CalendarCompareParams,
    ) -> Result<CalendarComparison, ApiError> {
        let year = params.hijri_year;
        if !COMPUTED_YEARS.contains(&year) {
            return Err(ApiError::invalid_params(
                "hijriYear must be between 1 and 9999",
            ));
        }
        let eph_id = params.ephemeris_id.as_deref();
        let (uaq, tabular): (Vec<_>, Vec<_>) = calendars()
            .iter()
            .partition(|i| i.kind == CalendarKind::UmmAlQura);
        let mut resolved: Vec<Resolved> = uaq.into_iter().map(Resolved::Builtin).collect();
        if !params.builtin_only {
            if let Some(v) = &params.visibility {
                resolved.push(Resolved::Computed(resolve_visibility(v)?));
            }
            resolved.extend(
                RuleCalendar::ALL
                    .into_iter()
                    .map(|r| Resolved::Computed(Computed::Rule(r))),
            );
        }
        resolved.extend(tabular.into_iter().map(Resolved::Builtin));

        let mut missing = Vec::new();
        let mut cals = Vec::with_capacity(resolved.len());
        let mut columns = Vec::with_capacity(resolved.len());
        for r in &resolved {
            let (cal, m) = self.calendar(r, eph_id, eph, &[year]);
            missing.extend(m);
            cals.push(cal);
            let d = r.description();
            columns.push(CompareColumn {
                id: d.id,
                label: d.short_label,
                name: d.name,
                description: d.description,
                computed: d.computed,
            });
        }
        let rows = (1..=12u8)
            .map(|month| {
                let starts: Vec<Option<MonthStart>> = cals
                    .iter()
                    .map(|c| {
                        c.get()
                            .month_start(year, month)
                            .map(|j| month_start(year, month, j))
                    })
                    .collect();
                let jdns: Vec<Option<i64>> = starts.iter().map(|s| s.map(|s| s.jdn)).collect();
                let mut best: Option<(i64, usize)> = None;
                for &s in jdns.iter().flatten() {
                    let count = jdns.iter().filter(|&&o| o == Some(s)).count();
                    if best.is_none_or(|(_, n)| count > n) {
                        best = Some((s, count));
                    }
                }
                let consensus = best.map(|(s, _)| s);
                let differs: Vec<bool> = jdns
                    .iter()
                    .map(|s| s.is_some() && *s != consensus)
                    .collect();
                CompareRow {
                    month,
                    starts,
                    consensus,
                    disagrees: differs.iter().any(|&d| d),
                    differs,
                }
            })
            .collect();
        Ok(CalendarComparison {
            hijri_year: year,
            columns,
            rows,
            missing,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::Location;

    fn eph() -> Arc<dyn Ephemeris> {
        crate::ephemeris::meeus(false)
    }

    fn spec(id: &str) -> CalendarSpec {
        CalendarSpec {
            id: id.into(),
            visibility: None,
        }
    }

    fn mecca_visibility() -> VisibilityCalendarSpec {
        VisibilityCalendarSpec {
            criterion_id: "yallop".into(),
            max_zone: 1,
            observers: OBSERVERS_MECCA.into(),
            location: None,
            location_name: None,
            grid_step_deg: None,
        }
    }

    fn d(year: i32, month: u32, day: u32) -> CivilDate {
        CivilDate { year, month, day }
    }

    #[test]
    fn catalog_lists_every_calendar() {
        let c = calendar_list(&CalendarListParams::default());
        assert_eq!(
            c.calendars.len(),
            calendars().len() + 1 + RuleCalendar::ALL.len()
        );
        assert_eq!(c.calendars[0].id, "umm-al-qura");
        assert_eq!(c.default_id, "umm-al-qura");
        assert!(
            c.calendars
                .iter()
                .any(|d| d.id == VISIBILITY_ID && d.computed)
        );
        assert_eq!(c.hijri_months[8].arabic, "رمضان");
        assert_eq!(c.hijri_months[8].name, "Ramaḍān");
        assert_eq!(c.weekdays[5].english, "Friday");
        assert_eq!(c.regions.len(), 4);
        let named = calendar_list(&CalendarListParams {
            visibility: Some(mecca_visibility()),
        });
        let v = named
            .calendars
            .iter()
            .find(|d| d.id == VISIBILITY_ID)
            .unwrap();
        assert_eq!(v.name, "Visibility: Yallop (1997) ≤ B, Mecca");
    }

    #[test]
    fn ramadan_1445_umm_al_qura() {
        // 1 Ramaḍān 1445 = Monday 11 March 2024; 30 days (Eid 10 April).
        let e = CalendarEngine::new();
        let m = e
            .month(
                &*eph(),
                &CalendarMonthParams {
                    calendar: spec("umm-al-qura"),
                    primary: PrimaryCalendar::Hijri,
                    month: None,
                    date: Some(d(2024, 3, 20)),
                    week_start: Some(6),
                    ephemeris_id: None,
                },
            )
            .unwrap();
        assert_eq!(
            m.month,
            MonthRef {
                year: 1445,
                month: 9
            }
        );
        assert!(m.missing.is_empty());
        assert_eq!(m.columns, vec![6, 0, 1, 2, 3, 4, 5]);
        let days: Vec<&CalendarDay> = m.weeks.iter().flatten().flatten().collect();
        assert_eq!(days.len(), 30);
        assert_eq!(days[0].civil, d(2024, 3, 11));
        assert_eq!(days[0].weekday, 1);
        assert_eq!(m.weeks[0][2].unwrap().civil, d(2024, 3, 11));
        assert_eq!(
            m.other_months,
            vec![
                MonthRef {
                    year: 2024,
                    month: 3
                },
                MonthRef {
                    year: 2024,
                    month: 4
                }
            ]
        );
        assert_eq!(
            m.next,
            MonthRef {
                year: 1445,
                month: 10
            }
        );
        assert_eq!(
            step_month(
                MonthRef {
                    year: 1445,
                    month: 1
                },
                -1
            ),
            MonthRef {
                year: 1444,
                month: 12
            }
        );
    }

    #[test]
    fn gregorian_view_and_range_messages() {
        let e = CalendarEngine::new();
        let m = e
            .month(
                &*eph(),
                &CalendarMonthParams {
                    calendar: spec("umm-al-qura"),
                    primary: PrimaryCalendar::Gregorian,
                    month: Some(MonthRef {
                        year: 2024,
                        month: 7,
                    }),
                    date: None,
                    week_start: Some(1),
                    ephemeris_id: None,
                },
            )
            .unwrap();
        let days: Vec<&CalendarDay> = m.weeks.iter().flatten().flatten().collect();
        assert_eq!(days.len(), 31);
        assert_eq!(
            days[6].hijri,
            Some(HijriYmd {
                year: 1446,
                month: 1,
                day: 1
            })
        );
        // Outside the official table.
        let m = e
            .month(
                &*eph(),
                &CalendarMonthParams {
                    calendar: spec("umm-al-qura"),
                    primary: PrimaryCalendar::Hijri,
                    month: Some(MonthRef {
                        year: 1600,
                        month: 1,
                    }),
                    date: None,
                    week_start: None,
                    ephemeris_id: None,
                },
            )
            .unwrap();
        assert!(m.weeks.is_empty());
        assert!(m.message.unwrap().contains("1343–1500"));
    }

    #[test]
    fn converter_both_ways() {
        let e = CalendarEngine::new();
        let to_hijri = e
            .convert(
                &*eph(),
                &CalendarConvertParams {
                    calendar: spec("umm-al-qura"),
                    civil: Some(d(2024, 7, 7)),
                    hijri: None,
                    ephemeris_id: None,
                },
            )
            .unwrap();
        assert_eq!(
            to_hijri.day.unwrap().hijri,
            Some(HijriYmd {
                year: 1446,
                month: 1,
                day: 1
            })
        );
        let back = e
            .convert(
                &*eph(),
                &CalendarConvertParams {
                    calendar: spec("umm-al-qura"),
                    civil: None,
                    hijri: Some(HijriYmd {
                        year: 1445,
                        month: 9,
                        day: 1,
                    }),
                    ephemeris_id: None,
                },
            )
            .unwrap();
        let day = back.day.unwrap();
        assert_eq!(day.civil, d(2024, 3, 11));
        assert_eq!(back.month_length, Some(30));
        // 30 Ramaḍān 1446 does not exist in Umm al-Qura (29 days).
        let bad = e
            .convert(
                &*eph(),
                &CalendarConvertParams {
                    calendar: spec("umm-al-qura"),
                    civil: None,
                    hijri: Some(HijriYmd {
                        year: 1446,
                        month: 9,
                        day: 30,
                    }),
                    ephemeris_id: None,
                },
            )
            .unwrap();
        assert!(bad.day.is_none());
        assert!(bad.message.unwrap().contains("29 days"));
        let none = e
            .convert(
                &*eph(),
                &CalendarConvertParams {
                    calendar: spec("umm-al-qura"),
                    civil: Some(d(2026, 2, 30)),
                    hijri: None,
                    ephemeris_id: None,
                },
            )
            .unwrap();
        assert_eq!(none.message.as_deref(), Some("No such date."));
        assert!(
            e.convert(
                &*eph(),
                &CalendarConvertParams {
                    calendar: spec("umm-al-qura"),
                    civil: None,
                    hijri: None,
                    ephemeris_id: None,
                }
            )
            .is_err()
        );
    }

    #[test]
    fn computed_calendars_report_missing_years_then_fill_them() {
        let eph = eph();
        let e = CalendarEngine::new();
        let params = CalendarMonthParams {
            calendar: spec("rule-umm-al-qura"),
            primary: PrimaryCalendar::Hijri,
            month: Some(MonthRef {
                year: 1445,
                month: 9,
            }),
            date: None,
            week_start: None,
            ephemeris_id: None,
        };
        let first = e.month(&*eph, &params).unwrap();
        assert!(first.weeks.is_empty());
        assert_eq!(
            first.missing,
            vec![MissingYear {
                calendar_id: "rule-umm-al-qura".into(),
                hijri_year: 1445
            }]
        );
        e.compute_missing(&eph, None, None, &first.missing).unwrap();
        let done = e.month(&*eph, &params).unwrap();
        assert!(done.missing.is_empty());
        let days: Vec<&CalendarDay> = done.weeks.iter().flatten().flatten().collect();
        // The computed Umm al-Qura rule agrees with the published calendar
        // for Ramaḍān 1445: 11 March 2024, 30 days.
        assert_eq!(days[0].civil, d(2024, 3, 11));
        assert_eq!(days.len(), 30);

        let year = e
            .compute_year(
                &eph,
                &MonthStartsParams {
                    calendar: spec("rule-umm-al-qura"),
                    hijri_year: 1445,
                    ephemeris_id: None,
                },
            )
            .unwrap();
        assert_eq!(year.starts.len(), 13);
        assert_eq!(year.starts[12].year, 1446);
        assert_eq!(year.starts[12].civil, d(2024, 7, 7));
    }

    #[test]
    fn visibility_calendar_at_a_location() {
        let eph = eph();
        let e = CalendarEngine::new();
        let v = VisibilityCalendarSpec {
            criterion_id: "yallop".into(),
            max_zone: 1,
            observers: OBSERVERS_LOCATION.into(),
            location: Some(Location {
                lat_deg: 21.4225,
                lon_deg: 39.8262,
                elevation_m: 0.0,
            }),
            location_name: Some("Makkah".into()),
            grid_step_deg: None,
        };
        let year = e
            .compute_year(
                &eph,
                &MonthStartsParams {
                    calendar: CalendarSpec {
                        id: VISIBILITY_ID.into(),
                        visibility: Some(v.clone()),
                    },
                    hijri_year: 1445,
                    ephemeris_id: None,
                },
            )
            .unwrap();
        assert_eq!(year.calendar_name, "Visibility: Yallop (1997) ≤ B, Makkah");
        assert_eq!(year.starts.len(), 13);
        for w in year.starts.windows(2) {
            assert!(matches!(w[1].jdn - w[0].jdn, 29 | 30));
        }
        // Ramaḍān 1445 by naked-eye visibility at Mecca: the crescent of
        // 10 March 2024 was hard (Yallop C/D), of 11 March easy, so the
        // month starts on 11 or 12 March.
        let ramadan = year.starts[8];
        assert!(
            [d(2024, 3, 11), d(2024, 3, 12)].contains(&ramadan.civil),
            "{:?}",
            ramadan.civil
        );
        // Bad settings.
        let bad = VisibilityCalendarSpec {
            max_zone: 9,
            ..v.clone()
        };
        assert!(resolve_visibility(&bad).is_err());
        let no_loc = VisibilityCalendarSpec {
            location: None,
            ..v
        };
        assert!(resolve_visibility(&no_loc).is_err());
    }

    #[test]
    fn comparison_highlights_disagreements() {
        let eph = eph();
        let e = CalendarEngine::new();
        let params = CalendarCompareParams {
            hijri_year: 1445,
            visibility: Some(mecca_visibility()),
            builtin_only: false,
            ephemeris_id: None,
        };
        let first = e.compare(&*eph, &params).unwrap();
        assert_eq!(first.missing.len(), 1 + RuleCalendar::ALL.len());
        assert_eq!(first.columns[0].id, "umm-al-qura");
        assert!(first.columns[2].computed);
        e.compute_missing(&eph, None, params.visibility.as_ref(), &first.missing)
            .unwrap();
        let c = e.compare(&*eph, &params).unwrap();
        assert!(c.missing.is_empty());
        assert_eq!(c.rows.len(), 12);
        for row in &c.rows {
            assert_eq!(row.starts.len(), c.columns.len());
            assert!(row.starts.iter().all(Option::is_some));
            assert_eq!(row.disagrees, row.differs.iter().any(|&x| x));
        }
        // The arithmetical calendars disagree with Umm al-Qura somewhere in
        // the year.
        assert!(c.rows.iter().any(|r| r.disagrees));
        let ramadan = &c.rows[8];
        assert_eq!(ramadan.starts[0].unwrap().civil, d(2024, 3, 11));
        // Built-ins only.
        let b = e
            .compare(
                &*eph,
                &CalendarCompareParams {
                    builtin_only: true,
                    ..params
                },
            )
            .unwrap();
        assert_eq!(b.columns.len(), calendars().len());
        assert!(b.missing.is_empty());
    }

    #[test]
    fn wire_shapes() {
        let p: CalendarMonthParams = serde_json::from_value(serde_json::json!({
            "calendar": {
                "id": "visibility",
                "visibility": {
                    "criterionId": "odeh", "maxZone": 1, "observers": "americas",
                    "gridStepDeg": 10
                }
            },
            "primary": "gregorian",
            "date": { "year": 2024, "month": 3, "day": 10 }
        }))
        .unwrap();
        assert_eq!(p.primary, PrimaryCalendar::Gregorian);
        assert_eq!(p.week_start, None);
        assert!(resolve(&p.calendar).is_ok());
        let v = serde_json::to_value(calendar_list(&CalendarListParams::default())).unwrap();
        assert_eq!(v["calendars"][0]["family"], "umm-al-qura");
        assert!(v["calendars"][0].get("shortLabel").is_some());
        assert!(v.get("dayConvention").is_some());
    }

    #[test]
    fn unknown_specs_are_rejected() {
        assert!(resolve(&spec("gregorian")).is_err());
        assert!(resolve(&spec(VISIBILITY_ID)).is_err());
        let e = CalendarEngine::new();
        assert!(
            e.compute_year(
                &eph(),
                &MonthStartsParams {
                    calendar: spec("rule-umm-al-qura"),
                    hijri_year: 3000, // after 3000 CE, beyond Meeus
                    ephemeris_id: None,
                }
            )
            .is_err()
        );
    }

    /// Meeus claiming coverage of 2022-06-01 to 2024-03-31 only (TT): from
    /// before 1 Muharram 1444, which the chain of 1445 AH starts from.
    struct HalfYear;

    impl HalfYear {
        fn ok(t: f64) -> bool {
            (2_459_731.5..=2_460_400.5).contains(&t)
        }
    }

    impl Ephemeris for HalfYear {
        fn name(&self) -> &str {
            "half year"
        }
        fn valid_range(&self) -> (f64, f64) {
            (2_459_731.5, 2_460_400.5)
        }
        fn sun_equatorial(&self, t: f64) -> mooncalc_core::Equatorial {
            let e = mooncalc_core::MeeusEphemeris::new().sun_equatorial(t);
            if Self::ok(t) { e } else { nan_equatorial() }
        }
        fn moon_equatorial(&self, t: f64) -> mooncalc_core::Equatorial {
            let e = mooncalc_core::MeeusEphemeris::new().moon_equatorial(t);
            if Self::ok(t) { e } else { nan_equatorial() }
        }
        fn sun_ecliptic(&self, t: f64) -> mooncalc_core::Ecliptic {
            let e = mooncalc_core::MeeusEphemeris::new().sun_ecliptic(t);
            if Self::ok(t) { e } else { nan_ecliptic() }
        }
        fn moon_ecliptic(&self, t: f64) -> mooncalc_core::Ecliptic {
            let e = mooncalc_core::MeeusEphemeris::new().moon_ecliptic(t);
            if Self::ok(t) { e } else { nan_ecliptic() }
        }
    }

    fn nan_equatorial() -> mooncalc_core::Equatorial {
        mooncalc_core::Equatorial {
            ra: f64::NAN,
            dec: f64::NAN,
            distance_km: f64::NAN,
        }
    }

    fn nan_ecliptic() -> mooncalc_core::Ecliptic {
        mooncalc_core::Ecliptic {
            lon: f64::NAN,
            lat: f64::NAN,
            distance_km: f64::NAN,
        }
    }

    /// A year the ephemeris covers only partly gives its covered months
    /// (agreeing with the Umm al-Qura table), not lengthened ones; a year
    /// outside it is an error and never listed as missing.
    #[test]
    fn partly_covered_years_keep_only_covered_months() {
        let eph: Arc<dyn Ephemeris> = Arc::new(HalfYear);
        // 1445 AH runs from July 2023 to July 2024: partly covered. 1446 AH
        // (from July 2024) is outside.
        assert_eq!(hijri_year_coverage(&*eph, 1445), YearCoverage::Partial);
        assert_eq!(hijri_year_coverage(&*eph, 1446), YearCoverage::Outside);
        assert_eq!(hijri_year_coverage(&*self::eph(), 1445), YearCoverage::Full);
        let e = CalendarEngine::new();
        let params = |hijri_year| MonthStartsParams {
            calendar: spec("rule-umm-al-qura"),
            hijri_year,
            ephemeris_id: None,
        };
        let y = e.compute_year(&eph, &params(1445)).unwrap();
        // Muharram (2023-07-19) to Ramadan (2024-03-11) start inside;
        // Shawwal (2024-04-10) does not.
        let months: Vec<u8> = y.starts.iter().map(|s| s.month).collect();
        assert_eq!(months, (1..=9).collect::<Vec<u8>>(), "{months:?}");
        for s in &y.starts {
            assert_eq!(
                mooncalc_hijri::UMM_AL_QURA.month_start(s.year, s.month),
                Some(s.jdn)
            );
        }
        assert!(e.compute_year(&eph, &params(1446)).is_err());
        let missing = e
            .missing_years(&*eph, &spec("rule-umm-al-qura"), None, &[1445, 1446, 1447])
            .unwrap();
        assert!(missing.is_empty(), "{missing:?}");
        let fresh = CalendarEngine::new();
        let missing = fresh
            .missing_years(&*eph, &spec("rule-umm-al-qura"), None, &[1446, 1445, 1445])
            .unwrap();
        assert_eq!(missing, [1445]);
    }

    #[test]
    fn computing_a_year_reports_progress_and_can_be_cancelled() {
        let e = CalendarEngine::new();
        let params = MonthStartsParams {
            calendar: spec("rule-conjunction-before-noon-ut"),
            hijri_year: 1445,
            ephemeris_id: None,
        };
        let mut calls = 0;
        let r = e.compute_year_with(&eph(), &params, |done, total| {
            calls += 1;
            assert_eq!(total, 13);
            done < 3
        });
        assert_eq!(r.unwrap_err().code, "cancelled");
        assert_eq!(calls, 3);
        // Nothing was kept: the view calendar is still empty.
        let view = e.view_calendar(&*eph(), &params.calendar, None).unwrap();
        assert_eq!(view.get().month_start(1445, 1), None);
        let mut last = (0, 0);
        let y = e
            .compute_year_with(&eph(), &params, |done, total| {
                last = (done, total);
                true
            })
            .unwrap();
        assert_eq!(last, (13, 13));
        assert_eq!(y.starts.len(), 13);
        assert!(view.get().month_start(1445, 1).is_none(), "a snapshot");
        let view = e.view_calendar(&*eph(), &params.calendar, None).unwrap();
        assert_eq!(view.get().month_start(1445, 1), Some(y.starts[0].jdn));
    }
}
