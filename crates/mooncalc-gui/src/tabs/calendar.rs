//! **Calendar** tab: Hijri calendars from [`mooncalc_hijri`].
//!
//! * A calendar selector over [`mooncalc_hijri::calendars`] (persisted in
//!   [`CalendarSettings`]).
//! * A month view, either a Hijri month with Gregorian dates or a Gregorian
//!   month with Hijri dates. Clicking a day sets the app's date (keeping the
//!   local time of day).
//! * A Gregorian ↔ Hijri converter.
//! * A comparison table: the start of every month of a Hijri year under
//!   every calendar, with disagreements highlighted (MoonCalc 6 offered a
//!   tabulated Hijri calendar).
//!
//! Civil dates use the Julian calendar before 15 October 1582 and the
//! Gregorian calendar from then on, like the date input
//! ([`crate::datetime`]). The app's date is taken in local time (UT plus the
//! time-zone offset).
//!
//! The Islamic day begins at sunset; like almanacs, every calendar here maps
//! a Hijri date to the civil day that begins after its first sunset (see
//! [`mooncalc_hijri::HijriCalendar`]).
//!
//! **Computed calendars.** Besides the built-in tables and arithmetical
//! calendars, the selector offers calendars computed with the selected
//! ephemeris ([`mooncalc_visibility::calendar`]):
//!
//! * a **visibility-based** calendar: a month starts after the evening of its
//!   29th day if the crescent is seen then under a criterion at a threshold
//!   zone, at the app's location, at Mecca, or anywhere in a region preset
//!   ([`VisibilityCalendarSettings`]); otherwise the month has 30 days;
//! * **rule-based** calendars ([`RuleCalendar`]): the Umm al-Qura rules and
//!   conjunction rules.
//!
//! Their month starts are computed by background jobs ([`crate::jobs`]),
//! a few Hijri years at a time, for the years on screen (the selected date,
//! the month view and the comparison table). Results are kept per
//! configuration (spec, ephemeris), and the providers keep their own caches,
//! so changing back to an earlier setting or year is instant. Until a year
//! is computed, the calendar reports it as outside its range. Every computed
//! calendar also gets a column in the comparison table.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

use mooncalc_core::{Ephemeris, GeoLocation};
use mooncalc_hijri::chained::{month_from_index, month_index};
use mooncalc_hijri::civil::{CivilCalendar, CivilDate};
use mooncalc_hijri::tabular::TABULAR_II_CIVIL;
use mooncalc_hijri::{
    CalendarInfo, CalendarKind, DEFAULT_CALENDAR_ID, GridDay, HijriCalendar, HijriDate, HijriMonth,
    MonthGrid, MonthStartProvider, ObservedCalendar, Weekday, calendars, civil_month_grid,
    hijri_month_grid, info_by_id,
};
use mooncalc_visibility::calendar::{
    DynProvider, MECCA, Observers, RegionPreset, RuleCalendar, VisibilityProvider,
};
use serde::{Deserialize, Serialize};

use crate::backend::{self, BackendChoice};
use crate::datetime;
use crate::jobs::{JobContext, JobError, JobSlot};
use crate::settings::{LocationSettings, Settings, TimeSettings};

use super::TabContext;

/// Civil calendar used for display: Julian before the 1582 reform, Gregorian
/// after, matching the date input.
pub const CIVIL: CivilCalendar = CivilCalendar::JulianGregorian;

const CIVIL_MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Short Hijri month labels for the month-view cells.
const HIJRI_MONTH_ABBR: [&str; 12] = [
    "Muh", "Saf", "Rab I", "Rab II", "Jum I", "Jum II", "Raj", "Sha", "Ram", "Shw", "Dhu Q",
    "Dhu H",
];

/// Characters the transliterated month names need beyond basic Latin.
const DIACRITIC_SAMPLE: &str = "ḌḍḤḥṢṣĀāĪīŪūʿ";

/// Shown for days a calendar cannot convert (outside its range, or a
/// computed calendar's years not computed yet).
const OUTSIDE_RANGE: &str = "outside this calendar's range (or not computed yet)";

const SUNSET_NOTE: &str = "The Islamic day begins at sunset. As in almanacs and official \
calendars, each Hijri date is shown on the civil day (midnight to midnight) that begins after \
its first sunset: the evening before belongs to it already.";

/// Which calendar the month view is laid out by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PrimaryCalendar {
    #[default]
    Hijri,
    Gregorian,
}

/// First column of the month view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WeekStart {
    #[default]
    Saturday,
    Sunday,
    Monday,
}

impl WeekStart {
    pub const ALL: [WeekStart; 3] = [WeekStart::Saturday, WeekStart::Sunday, WeekStart::Monday];

    pub fn weekday(self) -> Weekday {
        match self {
            WeekStart::Saturday => Weekday::Saturday,
            WeekStart::Sunday => Weekday::Sunday,
            WeekStart::Monday => Weekday::Monday,
        }
    }
}

/// Persisted Calendar-tab settings (`Settings::calendar`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CalendarSettings {
    /// Selected calendar id ([`CalendarInfo::id`]).
    pub calendar_id: String,
    pub primary: PrimaryCalendar,
    pub week_start: WeekStart,
    /// Configuration of the visibility-based calendar.
    pub visibility: VisibilityCalendarSettings,
}

impl Default for CalendarSettings {
    fn default() -> Self {
        Self {
            calendar_id: DEFAULT_CALENDAR_ID.into(),
            primary: PrimaryCalendar::Hijri,
            week_start: WeekStart::Saturday,
            visibility: VisibilityCalendarSettings::default(),
        }
    }
}

/// Id of the visibility-based calendar in [`CalendarSettings::calendar_id`].
pub const VISIBILITY_ID: &str = "visibility";

/// [`VisibilityCalendarSettings::observers`]: the app's observer location.
pub const OBSERVERS_APP_LOCATION: &str = "location";
/// [`VisibilityCalendarSettings::observers`]: the Kaʿba, Mecca.
pub const OBSERVERS_MECCA: &str = "mecca";

/// Settings of the visibility-based calendar.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VisibilityCalendarSettings {
    /// `Criterion::id`.
    pub criterion_id: String,
    /// Threshold: the crescent counts as seen in this zone or a better one
    /// (0 = the criterion's most visible zone).
    pub max_zone: usize,
    /// [`OBSERVERS_APP_LOCATION`], [`OBSERVERS_MECCA`] or a
    /// [`RegionPreset::id`] ("visible anywhere in the region").
    pub observers: String,
    /// Grid spacing for regions, degrees.
    pub grid_step_deg: f64,
}

impl Default for VisibilityCalendarSettings {
    /// Yallop zone B or better ("visible under perfect conditions", i.e.
    /// naked eye) at the app's location.
    fn default() -> Self {
        Self {
            criterion_id: "yallop".into(),
            max_zone: 1,
            observers: OBSERVERS_APP_LOCATION.into(),
            grid_step_deg: 5.0,
        }
    }
}

/// The registry entry for `id`, or the default calendar for an unknown id.
pub fn selected_info(id: &str) -> &'static CalendarInfo {
    info_by_id(id)
        .or_else(|| info_by_id(DEFAULT_CALENDAR_ID))
        .unwrap_or(&calendars()[0])
}

/// Whether `id` names a computed calendar (visibility- or rule-based).
pub fn is_computed_id(id: &str) -> bool {
    id == VISIBILITY_ID || RuleCalendar::from_id(id).is_some()
}

/// A calendar computed with the ephemeris.
#[derive(Clone, Debug, PartialEq)]
pub enum ComputedSpec {
    /// Crescent visibility.
    Visibility {
        criterion_id: String,
        max_zone: usize,
        observers: Observers,
        /// Label of the observers, for display.
        place: String,
    },
    /// An astronomical rule.
    Rule(RuleCalendar),
}

impl ComputedSpec {
    /// The visibility-based calendar configured in `vs`; `location` is the
    /// app's observer.
    pub fn visibility(vs: &VisibilityCalendarSettings, location: &LocationSettings) -> Self {
        let (observers, place) = match vs.observers.as_str() {
            OBSERVERS_MECCA => (Observers::Place(MECCA), "Mecca".to_owned()),
            id => match RegionPreset::from_id(id) {
                Some(r) => (
                    Observers::Region {
                        region: r.region(),
                        step_deg: vs.grid_step_deg.clamp(1.0, 30.0),
                    },
                    format!("anywhere in {}", r.name()),
                ),
                None => (
                    Observers::Place(GeoLocation::new(
                        location.lat_deg,
                        location.lon_deg,
                        location.elevation_m,
                    )),
                    location.name.clone(),
                ),
            },
        };
        ComputedSpec::Visibility {
            criterion_id: vs.criterion_id.clone(),
            max_zone: vs.max_zone,
            observers,
            place,
        }
    }

    /// The calendar id ([`VISIBILITY_ID`] or the rule's id).
    pub fn id(&self) -> &'static str {
        match self {
            ComputedSpec::Visibility { .. } => VISIBILITY_ID,
            ComputedSpec::Rule(r) => r.id(),
        }
    }

    /// Display name, e.g. "Visibility: Yallop (1997) ≤ B, Mecca".
    pub fn name(&self) -> String {
        match self {
            ComputedSpec::Visibility {
                criterion_id,
                max_zone,
                place,
                ..
            } => {
                let (name, zone) = match mooncalc_visibility::criteria::by_id(criterion_id) {
                    Some(c) => (
                        c.name().to_owned(),
                        c.zones()
                            .get(*max_zone)
                            .map_or("?".to_owned(), |z| z.code.to_owned()),
                    ),
                    None => (criterion_id.clone(), max_zone.to_string()),
                };
                format!("Visibility: {name} ≤ {zone}, {place}")
            }
            ComputedSpec::Rule(r) => r.name().to_owned(),
        }
    }

    /// One-sentence description.
    pub fn description(&self) -> &'static str {
        match self {
            ComputedSpec::Visibility { .. } => {
                "A month starts the day after the evening of its 29th day if the crescent is \
                 seen that evening (the criterion reaches the threshold zone or better); \
                 otherwise the month completes 30 days. Computed with the selected ephemeris."
            }
            ComputedSpec::Rule(r) => r.description(),
        }
    }

    /// A short column label for the comparison table.
    pub fn short_label(&self) -> &'static str {
        match self {
            ComputedSpec::Visibility { .. } => "Visibility",
            ComputedSpec::Rule(RuleCalendar::UmmAlQuraRule) => "UQ rule",
            ComputedSpec::Rule(RuleCalendar::ConjunctionBeforeSunsetMecca) => "Conj<sunset",
            ComputedSpec::Rule(RuleCalendar::ConjunctionBeforeNoonUt) => "Conj<12 UT",
        }
    }

    /// Cache key of this calendar computed with an ephemeris.
    fn key(&self, backend: BackendChoice, de440_path: &str) -> String {
        let spec = match self {
            ComputedSpec::Visibility {
                criterion_id,
                max_zone,
                observers,
                ..
            } => format!("visibility|{criterion_id}|{max_zone}|{observers:?}"),
            ComputedSpec::Rule(r) => r.id().to_owned(),
        };
        format!("{spec}|{:?}|{}", backend.effective(), de440_path.trim())
    }

    /// The month-start provider, computed with `eph`.
    fn provider(&self, eph: Arc<dyn Ephemeris>) -> Result<DynProvider, String> {
        match self {
            ComputedSpec::Visibility {
                criterion_id,
                max_zone,
                observers,
                ..
            } => {
                let criterion = mooncalc_visibility::criteria::by_id(criterion_id)
                    .ok_or_else(|| format!("Unknown criterion {criterion_id:?}"))?;
                Ok(Box::new(VisibilityProvider::new(
                    eph,
                    Arc::from(criterion),
                    *max_zone,
                    observers,
                )))
            }
            ComputedSpec::Rule(r) => Ok(r.provider(eph)),
        }
    }
}

/// Every computed calendar for the current settings: the configured
/// visibility-based calendar, then the rule-based ones.
pub fn computed_specs(settings: &Settings) -> Vec<ComputedSpec> {
    std::iter::once(ComputedSpec::visibility(
        &settings.calendar.visibility,
        &settings.location,
    ))
    .chain(RuleCalendar::ALL.into_iter().map(ComputedSpec::Rule))
    .collect()
}

/// Month starts known so far, as a provider (`None` for months not yet
/// computed).
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

/// The calendar the tab shows: a built-in one, or a computed one with the
/// years computed so far.
#[derive(Clone)]
pub enum ActiveCalendar {
    Builtin(&'static (dyn HijriCalendar + Sync)),
    Computed(Arc<ObservedCalendar<KnownStarts>>),
}

impl ActiveCalendar {
    pub fn get(&self) -> &dyn HijriCalendar {
        match self {
            ActiveCalendar::Builtin(c) => *c,
            ActiveCalendar::Computed(c) => c.as_ref(),
        }
    }
}

/// The built-in calendar for the selected id (the default one for a
/// computed or unknown id).
pub fn builtin_calendar(settings: &CalendarSettings) -> &'static (dyn HijriCalendar + Sync) {
    selected_info(&settings.calendar_id).calendar
}

/// Ephemeris and providers shared with the background jobs, so that their
/// caches outlive any one job.
#[derive(Default)]
struct Engine {
    ephemeris: Mutex<Option<(String, Arc<dyn Ephemeris>)>>,
    providers: Mutex<HashMap<String, Arc<DynProvider>>>,
}

/// Providers kept at most (all are dropped when this is exceeded).
const MAX_PROVIDERS: usize = 24;

impl Engine {
    fn ephemeris(
        &self,
        backend: BackendChoice,
        de440_path: &str,
    ) -> Result<Arc<dyn Ephemeris>, String> {
        let key = format!("{:?}|{}", backend.effective(), de440_path.trim());
        let mut slot = self.ephemeris.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((k, eph)) = slot.as_ref()
            && *k == key
        {
            return Ok(eph.clone());
        }
        // The shared backend (with its Meeus fallback for an unusable DE440).
        let eph = backend::resolve(backend, de440_path).eph;
        *slot = Some((key, eph.clone()));
        Ok(eph)
    }

    fn provider(
        &self,
        key: &str,
        spec: &ComputedSpec,
        eph: &Arc<dyn Ephemeris>,
    ) -> Result<Arc<DynProvider>, String> {
        if let Some(p) = self.lock_providers().get(key) {
            return Ok(p.clone());
        }
        let p = Arc::new(spec.provider(eph.clone())?);
        let mut map = self.lock_providers();
        if map.len() >= MAX_PROVIDERS {
            map.clear();
        }
        Ok(map.entry(key.to_owned()).or_insert(p).clone())
    }

    fn lock_providers(&self) -> std::sync::MutexGuard<'_, HashMap<String, Arc<DynProvider>>> {
        self.providers.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// One unit of background work: one Hijri year of one computed calendar.
#[derive(Clone, Debug, PartialEq)]
struct YearRequest {
    key: String,
    spec: ComputedSpec,
    year: i32,
}

/// What a job returns: per request (key, year), the month starts found
/// (month index → JDN), including the following Muḥarram.
type BatchResult = Vec<(String, i32, Vec<(i64, i64)>)>;

/// Requests per job: small batches deliver results progressively and lose
/// little when replaced.
const BATCH_SIZE: usize = 4;

/// Days before a month start whose ephemeris data the start depends on:
/// the conjunction (up to two days earlier), the evening tests from the day
/// before it, and each evening's searches (±1.75 d around local noon, which
/// is up to half a day from UT noon).
const START_MARGIN_BEFORE: f64 = 5.0;
/// Days after a month start whose data it depends on (its eve's searches).
const START_MARGIN_AFTER: f64 = 3.0;
/// Largest difference between the tabular estimate of a month start and
/// a computed one, days.
const TABULAR_ERROR_DAYS: i64 = 3;

/// Whether the ephemeris covers everything the month starting on civil day
/// `jdn` depends on. Starts too close to (or beyond) the edges of the
/// coverage are dropped rather than shown wrong: outside it the evening
/// tests see no crescent, which would silently lengthen months.
fn start_covered(eph: &dyn Ephemeris, jdn: i64) -> bool {
    let (a, b) = eph.valid_range();
    let jd = jdn as f64;
    jd - START_MARGIN_BEFORE >= a && jd + START_MARGIN_AFTER <= b
}

/// Whether a month whose tabular start is `estimate` may start inside the
/// coverage (otherwise it is not computed at all).
fn estimate_covered(eph: &dyn Ephemeris, estimate: i64) -> bool {
    start_covered(eph, estimate - TABULAR_ERROR_DAYS)
        || start_covered(eph, estimate + TABULAR_ERROR_DAYS)
}

/// How much of a Hijri year the ephemeris covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum YearCoverage {
    Full,
    Partial,
    Outside,
}

/// The coverage of Hijri `year`, from the tabular estimate of its months.
fn hijri_year_coverage(eph: &dyn Ephemeris, year: i32) -> YearCoverage {
    let starts: Vec<i64> = (1..=12)
        .filter_map(|m| TABULAR_II_CIVIL.month_start(year, m))
        .collect();
    let covered = starts
        .iter()
        .filter(|&&s| {
            start_covered(eph, s - TABULAR_ERROR_DAYS) && start_covered(eph, s + TABULAR_ERROR_DAYS)
        })
        .count();
    if covered == starts.len() {
        YearCoverage::Full
    } else if starts.iter().any(|&s| estimate_covered(eph, s)) {
        YearCoverage::Partial
    } else {
        YearCoverage::Outside
    }
}

/// A sentence about the wanted Hijri years the ephemeris does not fully
/// cover, if any.
fn coverage_note(active: &backend::ActiveEphemeris, years: &[i32]) -> Option<String> {
    let mut partial = Vec::new();
    let mut outside = Vec::new();
    let mut sorted = years.to_vec();
    sorted.sort_unstable();
    for y in sorted {
        match hijri_year_coverage(active.eph.as_ref(), y) {
            YearCoverage::Full => {}
            YearCoverage::Partial => partial.push(y.to_string()),
            YearCoverage::Outside => outside.push(y.to_string()),
        }
    }
    if partial.is_empty() && outside.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    if !partial.is_empty() {
        parts.push(format!(
            "Hijri {} {} only partly inside",
            if partial.len() == 1 { "year" } else { "years" },
            partial.join(", ")
        ));
    }
    if !outside.is_empty() {
        parts.push(format!(
            "{} {} outside",
            if outside.len() == 1 { "year" } else { "years" },
            outside.join(", ")
        ));
    }
    Some(format!(
        "Computed calendars: {} the {} coverage ({}). Months that start outside it or within \
         a few days of its edges are not computed, nor, for the calendars decided on the \
         29th day (visibility, Umm al-Qura rule), the months of the first Hijri year that \
         starts inside it.",
        parts.join("; "),
        active.used.short_label(),
        active.coverage_text()
    ))
}

/// Compute a batch of years on a worker thread.
fn compute_batch(
    engine: &Engine,
    backend: BackendChoice,
    de440_path: &str,
    batch: &[YearRequest],
    job: &JobContext,
) -> Result<BatchResult, JobError> {
    let eph = engine
        .ephemeris(backend, de440_path)
        .map_err(JobError::Failed)?;
    let total = batch.len() * 13;
    let mut out = Vec::with_capacity(batch.len());
    for (b, req) in batch.iter().enumerate() {
        let provider = engine
            .provider(&req.key, &req.spec, &eph)
            .map_err(JobError::Failed)?;
        let first = month_index(req.year, 1);
        let mut starts = Vec::with_capacity(13);
        for (i, n) in (first..=first + 12).enumerate() {
            job.checkpoint()?;
            let (y, m) = month_from_index(n);
            // Skip months entirely outside the coverage; keep only starts
            // whose data is all inside it.
            let estimate = TABULAR_II_CIVIL.month_start(y, m);
            if estimate.is_none_or(|e| estimate_covered(eph.as_ref(), e))
                && let Some(jdn) = provider.month_start(y, m)
                && start_covered(eph.as_ref(), jdn)
            {
                starts.push((n, jdn));
            }
            job.set_progress((b * 13 + i + 1) as f32 / total as f32);
        }
        out.push((req.key.clone(), req.year, starts));
    }
    Ok(out)
}

/// Results of one computed calendar received so far.
#[derive(Default)]
struct Computed {
    starts: BTreeMap<i64, i64>,
    years: BTreeSet<i32>,
    calendar: Option<Arc<ObservedCalendar<KnownStarts>>>,
}

impl Computed {
    fn add(&mut self, year: i32, starts: Vec<(i64, i64)>, spec: &ComputedSpec) {
        self.starts.extend(starts);
        self.years.insert(year);
        let (Some(&first), Some(&last)) = (self.years.first(), self.years.last()) else {
            return;
        };
        self.calendar = Some(Arc::new(ObservedCalendar::new(
            KnownStarts(self.starts.clone()),
            spec.id(),
            spec.name(),
            first,
            last,
        )));
    }
}

/// The Hijri year containing civil day `jdn`, near enough to choose which
/// years to compute (the tabular calendar is within a few days of any lunar
/// calendar).
fn approximate_hijri_year(jdn: i64) -> i32 {
    TABULAR_II_CIVIL.to_hijri(jdn).map_or(1448, |h| h.year)
}

/// The civil day (JDN) containing Julian Date `jd`.
pub fn civil_day(jd: f64) -> i64 {
    (jd + 0.5).floor() as i64
}

/// The selected civil day, in local time.
pub fn selected_day(time: &TimeSettings) -> i64 {
    civil_day(time.jd_local())
}

/// Today's civil day in the zone of `time`.
pub fn today(time: &TimeSettings) -> i64 {
    civil_day(time.zone().to_local(datetime::now_jd()))
}

/// Move `time` to local civil day `jdn`, keeping the local time of day.
pub fn set_day(time: &mut TimeSettings, jdn: i64) {
    let local = time.jd_local();
    let fraction = local + 0.5 - (local + 0.5).floor();
    time.jd_ut = time.zone().to_ut(jdn as f64 - 0.5 + fraction);
}

/// `11 November 2026`.
pub fn format_civil(date: CivilDate) -> String {
    format!(
        "{} {} {}",
        date.day,
        CIVIL_MONTHS[usize::from(date.month - 1)],
        date.year
    )
}

/// `11 Nov 2026`.
pub fn format_civil_short(date: CivilDate) -> String {
    format!(
        "{} {} {}",
        date.day,
        &CIVIL_MONTHS[usize::from(date.month - 1)][..3],
        date.year
    )
}

/// Month name, with diacritics if the fonts have them.
pub fn hijri_month_name(month: u8, diacritics: bool) -> &'static str {
    match HijriMonth::from_number(month) {
        Some(m) if diacritics => m.name(),
        Some(m) => m.name_ascii(),
        None => "?",
    }
}

/// `20 Jumādā al-Ūlā 1448 AH`.
pub fn format_hijri(date: HijriDate, diacritics: bool) -> String {
    format!(
        "{} {} {} AH",
        date.day,
        hijri_month_name(date.month, diacritics),
        date.year
    )
}

/// A month shown in the month view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MonthView {
    Hijri { year: i32, month: u8 },
    Civil { year: i32, month: u8 },
}

impl MonthView {
    /// The month of the given kind containing civil day `jdn`. A Hijri view
    /// of a day outside the calendar's range is clamped to its first or last
    /// month.
    pub fn containing(jdn: i64, primary: PrimaryCalendar, cal: &dyn HijriCalendar) -> Self {
        match primary {
            PrimaryCalendar::Gregorian => {
                let c = CivilDate::from_jdn(jdn, CIVIL);
                MonthView::Civil {
                    year: c.year,
                    month: c.month,
                }
            }
            PrimaryCalendar::Hijri => match cal.to_hijri(jdn) {
                Some(h) => MonthView::Hijri {
                    year: h.year,
                    month: h.month,
                },
                None => {
                    let range = cal.supported_range();
                    let before = cal
                        .supported_jdn_range()
                        .is_some_and(|(first, _)| jdn < first);
                    if before {
                        MonthView::Hijri {
                            year: range.first_year,
                            month: 1,
                        }
                    } else {
                        MonthView::Hijri {
                            year: range.last_year,
                            month: 12,
                        }
                    }
                }
            },
        }
    }

    /// The month `delta` months later (earlier if negative).
    pub fn step(self, delta: i32) -> Self {
        let shift = |year: i32, month: u8| {
            let index = year * 12 + i32::from(month) - 1 + delta;
            (index.div_euclid(12), (index.rem_euclid(12) + 1) as u8)
        };
        match self {
            MonthView::Hijri { year, month } => {
                let (year, month) = shift(year, month);
                MonthView::Hijri { year, month }
            }
            MonthView::Civil { year, month } => {
                let (year, month) = shift(year, month);
                MonthView::Civil { year, month }
            }
        }
    }

    /// The days of the month, or `None` outside the calendar's range.
    pub fn grid(self, cal: &dyn HijriCalendar) -> Option<MonthGrid> {
        match self {
            MonthView::Hijri { year, month } => hijri_month_grid(cal, year, month, CIVIL),
            MonthView::Civil { year, month } => civil_month_grid(cal, year, month, CIVIL),
        }
    }

    /// `Ramadan 1447 AH` or `November 2026`.
    pub fn title(self, diacritics: bool) -> String {
        match self {
            MonthView::Hijri { year, month } => {
                format!("{} {year} AH", hijri_month_name(month, diacritics))
            }
            MonthView::Civil { year, month } => {
                format!("{} {year}", CIVIL_MONTHS[usize::from(month - 1)])
            }
        }
    }
}

/// One row of the month-start comparison: the first day of `month` under
/// each compared calendar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComparisonRow {
    pub month: u8,
    /// Month start (JDN) per calendar, in the order given; `None` where the
    /// calendar does not cover the year.
    pub starts: Vec<Option<i64>>,
    /// The most common start (ties go to the earlier calendar in the list).
    pub consensus: Option<i64>,
}

impl ComparisonRow {
    /// Whether the calendars that cover the month disagree.
    pub fn disagrees(&self) -> bool {
        self.starts
            .iter()
            .flatten()
            .any(|&s| Some(s) != self.consensus)
    }

    /// Whether calendar `i` differs from the consensus.
    pub fn differs(&self, i: usize) -> bool {
        matches!(self.starts.get(i), Some(Some(s)) if Some(*s) != self.consensus)
    }
}

/// The start of every month of Hijri `year` under each of `calendars`.
pub fn compare_month_starts(year: i32, calendars: &[&dyn HijriCalendar]) -> Vec<ComparisonRow> {
    (1..=12)
        .map(|month| {
            let starts: Vec<Option<i64>> = calendars
                .iter()
                .map(|c| c.month_start(year, month))
                .collect();
            let mut best: Option<(i64, usize)> = None;
            for s in starts.iter().flatten() {
                let count = starts.iter().filter(|&&o| o == Some(*s)).count();
                if best.is_none_or(|(_, n)| count > n) {
                    best = Some((*s, count));
                }
            }
            ComparisonRow {
                month,
                starts,
                consensus: best.map(|(s, _)| s),
            }
        })
        .collect()
}

/// A short column label for the comparison table.
pub fn short_label(id: &str) -> &str {
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

/// One column of the comparison table.
struct Column<'a> {
    id: &'a str,
    label: String,
    hover: String,
    calendar: &'a dyn HijriCalendar,
    computed: bool,
}

/// Converter inputs.
#[derive(Debug, Clone, Copy)]
struct ConverterState {
    civil: (i32, u8, u8),
    hijri: (i32, u8, u8),
}

/// State of the Calendar tab.
#[derive(Default)]
pub struct CalendarTab {
    /// The month on screen; `None` until first drawn.
    view: Option<MonthView>,
    /// What the view was last centred on: (selected day, primary, calendar,
    /// whether the calendar covered the day).
    centred_on: Option<(i64, PrimaryCalendar, String, bool)>,
    converter: Option<ConverterState>,
    /// Hijri year of the comparison table; `None` follows the selected date.
    comparison_year: Option<i32>,
    /// Whether the fonts can show the transliteration diacritics.
    diacritics: Option<bool>,
    /// Ephemeris and month-start providers shared with the jobs.
    engine: Arc<Engine>,
    /// Computed month starts per calendar key.
    computed: HashMap<String, Computed>,
    job: JobSlot<BatchResult>,
    /// What the running job computes.
    in_flight: Vec<YearRequest>,
    /// The request that last failed; not retried until the request changes.
    failed: Option<Vec<YearRequest>>,
    error: Option<String>,
    /// Hijri years on screen, most important first (set by `ui`).
    wanted_years: Vec<i32>,
    /// Which wanted years the ephemeris does not cover (set by `poll`).
    coverage_note: Option<String>,
}

impl CalendarTab {
    /// The computed calendars for the current settings, with their keys.
    fn specs(settings: &Settings) -> Vec<(String, ComputedSpec)> {
        computed_specs(settings)
            .into_iter()
            .map(|s| (s.key(settings.backend, &settings.de440_path), s))
            .collect()
    }

    /// The months of `spec` computed so far, as a calendar (empty if none).
    fn computed_calendar(
        &self,
        key: &str,
        spec: &ComputedSpec,
    ) -> Arc<ObservedCalendar<KnownStarts>> {
        self.computed
            .get(key)
            .and_then(|c| c.calendar.clone())
            .unwrap_or_else(|| {
                Arc::new(ObservedCalendar::new(
                    KnownStarts::default(),
                    spec.id(),
                    spec.name(),
                    1,
                    0,
                ))
            })
    }

    /// The calendar the tab works with.
    pub fn active_calendar(&self, settings: &Settings) -> ActiveCalendar {
        let id = settings.calendar.calendar_id.as_str();
        if !is_computed_id(id) {
            return ActiveCalendar::Builtin(builtin_calendar(&settings.calendar));
        }
        match Self::specs(settings)
            .into_iter()
            .find(|(_, s)| s.id() == id)
        {
            Some((key, spec)) => ActiveCalendar::Computed(self.computed_calendar(&key, &spec)),
            None => ActiveCalendar::Builtin(builtin_calendar(&settings.calendar)),
        }
    }

    /// The (calendar, year) pairs on screen that have not been computed,
    /// the selected calendar first.
    fn missing(&self, settings: &Settings) -> Vec<YearRequest> {
        let selected = settings.calendar.calendar_id.as_str();
        let mut specs = Self::specs(settings);
        specs.sort_by_key(|(_, s)| s.id() != selected);
        let mut out = Vec::new();
        for (key, spec) in specs {
            for &year in &self.wanted_years {
                let done = self
                    .computed
                    .get(&key)
                    .is_some_and(|c| c.years.contains(&year));
                if !done {
                    out.push(YearRequest {
                        key: key.clone(),
                        spec: spec.clone(),
                        year,
                    });
                }
            }
        }
        out
    }

    /// Collect finished month starts and start jobs for the missing years.
    pub fn poll(&mut self, ctx: &egui::Context, tc: &mut TabContext<'_>) {
        match self.job.poll() {
            Some(Ok(results)) => {
                for (key, year, starts) in results {
                    if let Some(req) = self.in_flight.iter().find(|r| r.key == key) {
                        let spec = req.spec.clone();
                        self.computed
                            .entry(key)
                            .or_default()
                            .add(year, starts, &spec);
                    }
                }
                self.error = None;
            }
            Some(Err(e)) => {
                tc.status.error(format!("Calendar: {e}"));
                self.error = Some(e);
                self.failed = Some(self.missing(tc.settings));
            }
            None => {}
        }
        if !self.job.is_running() {
            self.in_flight.clear();
        }

        let missing = self.missing(tc.settings);
        // Replace a job whose work is no longer wanted (settings changed).
        if self.job.is_running() && self.in_flight.iter().any(|r| !missing.contains(r)) {
            self.job.cancel();
            self.in_flight.clear();
        }
        if !self.job.is_running() && !missing.is_empty() && self.failed.as_ref() != Some(&missing) {
            self.failed = None;
            let batch: Vec<YearRequest> = missing.into_iter().take(BATCH_SIZE).collect();
            self.in_flight = batch.clone();
            let engine = self.engine.clone();
            let backend = tc.settings.backend;
            let path = tc.settings.de440_path.clone();
            let repaint = ctx.clone();
            self.job.submit(
                "Computing Hijri month starts",
                move || repaint.request_repaint(),
                move |job| compute_batch(&engine, backend, &path, &batch, job),
            );
        }
        if let Some(progress) = self.job.progress() {
            tc.status.report_job(progress);
        }
        let active = backend::resolve(tc.settings.backend, &tc.settings.de440_path);
        self.coverage_note = coverage_note(&active, &self.wanted_years);
    }

    /// Remember which Hijri years are on screen, for [`Self::poll`].
    fn set_wanted_years(&mut self, settings: &Settings, cal: &dyn HijriCalendar) {
        let selected = selected_day(&settings.time);
        let mut years = Vec::new();
        // A few days either side, in case the computed calendar puts the
        // year boundary elsewhere than the tabular estimate.
        for jdn in [selected, selected - 3, selected + 3] {
            years.push(approximate_hijri_year(jdn));
        }
        let centre = years[0];
        let near = move |y: i32| (y - centre).abs() <= 300;
        if let Some(view) = self.view {
            match view {
                MonthView::Hijri { year, .. } if near(year) => years.push(year),
                MonthView::Civil { year, month } => {
                    if let Some(first) = CivilDate::new(year, month, 1).to_jdn(CIVIL) {
                        years.push(approximate_hijri_year(first - 3));
                        years.push(approximate_hijri_year(first + 34));
                    }
                }
                MonthView::Hijri { .. } => {}
            }
        }
        if let Some(y) = self.comparison_year.filter(|&y| near(y)) {
            years.push(y);
        } else {
            years.push(cal.to_hijri(selected).map_or(centre, |h| h.year));
        }
        if let Some(c) = &self.converter {
            if let Some(jdn) = CivilDate::new(c.civil.0, c.civil.1, c.civil.2).to_jdn(CIVIL) {
                years.push(approximate_hijri_year(jdn));
            }
            years.push(c.hijri.0);
        }
        let mut seen = BTreeSet::new();
        years.retain(|&y| (1..=9999).contains(&y) && near(y) && seen.insert(y));
        self.wanted_years = years;
    }

    /// Whether the ephemeris covers what a month starting on civil day `jdn`
    /// depends on (see [`start_covered`]).
    fn day_covered(&self, settings: &Settings, jdn: i64) -> bool {
        let active = backend::resolve(settings.backend, &settings.de440_path);
        start_covered(active.eph.as_ref(), jdn)
    }

    /// Progress or problems of the computed calendars, for the selector.
    fn computed_status(&self, ui: &mut egui::Ui, cal: &dyn HijriCalendar) {
        if let Some(e) = &self.error {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("Computation failed: {e}"),
            );
        } else if let Some(p) = self.job.progress() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(format!(
                    "Computing month starts… {:.0}%",
                    100.0 * p.fraction
                ));
            });
        } else if cal.supported_range().first_year > cal.supported_range().last_year {
            ui.weak("Month starts are computed for the years on screen.");
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        let diacritics = *self.diacritics.get_or_insert_with(|| {
            ui.ctx()
                .fonts_mut(|f| f.has_glyphs(&egui::FontId::proportional(14.0), DIACRITIC_SAMPLE))
        });
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Hijri calendar");
            calendar_selector(ui, tc.settings);
            let active = self.active_calendar(tc.settings);
            let cal = active.get();
            let computed = is_computed_id(&tc.settings.calendar.calendar_id);
            if computed {
                self.computed_status(ui, cal);
            }
            if let Some(note) = &self.coverage_note {
                ui.colored_label(ui.visuals().warn_fg_color, format!("⚠ {note}"));
            }
            ui.add(egui::Label::new(egui::RichText::new(SUNSET_NOTE).small().weak()).wrap());

            let selected = selected_day(&tc.settings.time);
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label("Selected date:");
                let civil = CivilDate::from_jdn(selected, CIVIL);
                let weekday = Weekday::from_jdn(selected).name_english();
                match cal.to_hijri(selected) {
                    Some(h) => ui.strong(format_hijri(h, diacritics)),
                    None if computed && !self.day_covered(tc.settings, selected) => {
                        ui.weak("outside the ephemeris coverage")
                    }
                    None => ui.weak(OUTSIDE_RANGE),
                };
                ui.label(format!("— {weekday} {} (local)", format_civil(civil)));
            });

            ui.add_space(6.0);
            egui::CollapsingHeader::new("Month view")
                .default_open(true)
                .show(ui, |ui| self.month_view(ui, tc, cal, diacritics));
            egui::CollapsingHeader::new("Date converter")
                .default_open(true)
                .show(ui, |ui| self.converter(ui, tc, cal, diacritics));
            egui::CollapsingHeader::new("Month starts under every calendar")
                .default_open(true)
                .show(ui, |ui| self.comparison(ui, tc, cal, diacritics));
            self.set_wanted_years(tc.settings, cal);
        });
    }

    fn month_view(
        &mut self,
        ui: &mut egui::Ui,
        tc: &mut TabContext<'_>,
        cal: &dyn HijriCalendar,
        diacritics: bool,
    ) {
        let cs = &mut tc.settings.calendar;
        ui.horizontal_wrapped(|ui| {
            ui.label("Lay out by:");
            ui.selectable_value(&mut cs.primary, PrimaryCalendar::Hijri, "Hijri month");
            ui.selectable_value(
                &mut cs.primary,
                PrimaryCalendar::Gregorian,
                "Gregorian month",
            );
            ui.separator();
            ui.label("Week starts:");
            egui::ComboBox::from_id_salt("week_start")
                .selected_text(cs.week_start.weekday().name_english())
                .show_ui(ui, |ui| {
                    for w in WeekStart::ALL {
                        ui.selectable_value(&mut cs.week_start, w, w.weekday().name_english());
                    }
                });
        });

        let primary = cs.primary;
        let week_start = cs.week_start.weekday();
        let selected = selected_day(&tc.settings.time);
        let today = today(&tc.settings.time);

        // Re-centre on the selected date when it, the layout or the
        // calendar changes; otherwise keep the month the user navigated to.
        let key = (
            selected,
            primary,
            cs.calendar_id.clone(),
            cal.to_hijri(selected).is_some(),
        );
        if self.centred_on.as_ref() != Some(&key) || self.view.is_none() {
            self.view = Some(MonthView::containing(selected, primary, cal));
            self.centred_on = Some(key);
        }
        let mut view = self
            .view
            .unwrap_or(MonthView::containing(selected, primary, cal));

        ui.horizontal(|ui| {
            if ui
                .button("« Prev")
                .on_hover_text("Previous month")
                .clicked()
            {
                view = view.step(-1);
            }
            if ui
                .button("Today")
                .on_hover_text("Show the current month")
                .clicked()
            {
                view = MonthView::containing(today, primary, cal);
            }
            if ui
                .button("Selected")
                .on_hover_text("Show the selected date's month")
                .clicked()
            {
                view = MonthView::containing(selected, primary, cal);
            }
            if ui.button("Next »").on_hover_text("Next month").clicked() {
                view = view.step(1);
            }
            ui.add_space(8.0);
            ui.strong(egui::RichText::new(view.title(diacritics)).size(18.0));
        });
        self.view = Some(view);

        let Some(grid) = view.grid(cal) else {
            ui.weak(format!("This month is {OUTSIDE_RANGE}."));
            return;
        };
        // The months on the other calendar this month touches.
        let subtitle = match view {
            MonthView::Hijri { .. } => {
                let months = grid.civil_months();
                months
                    .iter()
                    .map(|&(y, m)| format!("{} {y}", CIVIL_MONTHS[usize::from(m - 1)]))
                    .collect::<Vec<_>>()
                    .join(" – ")
            }
            MonthView::Civil { .. } => grid
                .hijri_months()
                .iter()
                .map(|&(y, m)| format!("{} {y}", hijri_month_name(m, diacritics)))
                .collect::<Vec<_>>()
                .join(" – "),
        };
        ui.weak(subtitle);

        if let Some(jdn) = month_grid(ui, &grid, view, week_start, selected, today, diacritics) {
            set_day(&mut tc.settings.time, jdn);
        }
    }

    fn converter(
        &mut self,
        ui: &mut egui::Ui,
        tc: &mut TabContext<'_>,
        cal: &dyn HijriCalendar,
        diacritics: bool,
    ) {
        let selected = selected_day(&tc.settings.time);
        let from_day = |jdn: i64| {
            let c = CivilDate::from_jdn(jdn, CIVIL);
            let h = cal.to_hijri(jdn).unwrap_or(HijriDate::new(1, 1, 1));
            ConverterState {
                civil: (c.year, c.month, c.day),
                hijri: (h.year, h.month, h.day),
            }
        };
        let state = self.converter.get_or_insert_with(|| from_day(selected));
        let mut set_app_day = None;

        egui::Grid::new("converter")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Gregorian");
                ui.horizontal(|ui| {
                    let (y, m, d) = &mut state.civil;
                    ui.add(egui::DragValue::new(d).range(1..=31).prefix("day "));
                    egui::ComboBox::from_id_salt("conv_civil_month")
                        .selected_text(CIVIL_MONTHS[usize::from(*m - 1)])
                        .show_ui(ui, |ui| {
                            for (i, name) in CIVIL_MONTHS.iter().enumerate() {
                                ui.selectable_value(m, i as u8 + 1, *name);
                            }
                        });
                    ui.add(egui::DragValue::new(y).range(-4712..=9999).speed(0.2));
                });
                ui.end_row();

                ui.label("→ Hijri");
                let (y, m, d) = state.civil;
                match CivilDate::new(y, m, d).to_jdn(CIVIL) {
                    None => {
                        ui.colored_label(ui.visuals().warn_fg_color, "No such date");
                    }
                    Some(jdn) => {
                        ui.horizontal(|ui| {
                            match cal.to_hijri(jdn) {
                                Some(h) => ui.strong(format!(
                                    "{} ({})",
                                    format_hijri(h, diacritics),
                                    Weekday::from_jdn(jdn).name_english()
                                )),
                                None => ui.weak(OUTSIDE_RANGE),
                            };
                            if ui.small_button("Set date").clicked() {
                                set_app_day = Some(jdn);
                            }
                        });
                    }
                }
                ui.end_row();

                ui.label("Hijri");
                ui.horizontal(|ui| {
                    let (y, m, d) = &mut state.hijri;
                    ui.add(egui::DragValue::new(d).range(1..=30).prefix("day "));
                    egui::ComboBox::from_id_salt("conv_hijri_month")
                        .selected_text(hijri_month_name(*m, diacritics))
                        .show_ui(ui, |ui| {
                            for month in 1..=12u8 {
                                ui.selectable_value(m, month, hijri_month_name(month, diacritics));
                            }
                        });
                    ui.add(
                        egui::DragValue::new(y)
                            .range(1..=9999)
                            .speed(0.2)
                            .suffix(" AH"),
                    );
                });
                ui.end_row();

                ui.label("→ Gregorian");
                let (y, m, d) = state.hijri;
                let date = HijriDate::new(y, m, d);
                match cal.from_hijri(date) {
                    Some(jdn) => {
                        ui.horizontal(|ui| {
                            ui.strong(format!(
                                "{} {}",
                                Weekday::from_jdn(jdn).name_english(),
                                format_civil(CivilDate::from_jdn(jdn, CIVIL))
                            ));
                            if ui.small_button("Set date").clicked() {
                                set_app_day = Some(jdn);
                            }
                        });
                    }
                    None => {
                        let range = cal.supported_range();
                        let msg = if range.contains(y) {
                            format!(
                                "No such date: this month has {} days",
                                cal.month_length(y, m).unwrap_or(0)
                            )
                        } else {
                            format!(
                                "Outside this calendar's range ({}–{} AH)",
                                range.first_year, range.last_year
                            )
                        };
                        ui.colored_label(ui.visuals().warn_fg_color, msg);
                    }
                }
                ui.end_row();
            });
        if ui
            .button("Fill from selected date")
            .on_hover_text("Load the app's current date into both converter fields")
            .clicked()
        {
            *state = from_day(selected);
        }
        if let Some(jdn) = set_app_day {
            set_day(&mut tc.settings.time, jdn);
        }
    }

    fn comparison(
        &mut self,
        ui: &mut egui::Ui,
        tc: &mut TabContext<'_>,
        cal: &dyn HijriCalendar,
        diacritics: bool,
    ) {
        let selected = selected_day(&tc.settings.time);
        let following = cal
            .to_hijri(selected)
            .map_or_else(|| approximate_hijri_year(selected), |h| h.year);
        let mut year = self.comparison_year.unwrap_or(following);
        ui.horizontal(|ui| {
            ui.label("Hijri year:");
            if ui
                .small_button("«")
                .on_hover_text("Previous year")
                .clicked()
            {
                year -= 1;
            }
            ui.add(
                egui::DragValue::new(&mut year)
                    .range(1..=9999)
                    .suffix(" AH"),
            );
            if ui.small_button("»").on_hover_text("Next year").clicked() {
                year += 1;
            }
            if self.comparison_year.is_some()
                && ui
                    .small_button("Follow selected date")
                    .on_hover_text("Show the Hijri year of the selected date")
                    .clicked()
            {
                year = following;
                self.comparison_year = None;
                return;
            }
            if year != following || self.comparison_year.is_some() {
                self.comparison_year = Some(year);
            }
        });
        let year = self.comparison_year.unwrap_or(following);

        // Columns: the Umm al-Qura tables, the computed calendars, then the
        // arithmetical calendars.
        let computed: Vec<(ComputedSpec, Arc<ObservedCalendar<KnownStarts>>)> =
            Self::specs(tc.settings)
                .into_iter()
                .map(|(key, spec)| {
                    let cal = self.computed_calendar(&key, &spec);
                    (spec, cal)
                })
                .collect();
        let mut columns: Vec<Column<'_>> = Vec::new();
        let (uaq, tabular): (Vec<_>, Vec<_>) = calendars()
            .iter()
            .partition(|i| i.kind == CalendarKind::UmmAlQura);
        let builtin = |i: &'static CalendarInfo| Column {
            id: i.id,
            label: short_label(i.id).to_owned(),
            hover: format!("{}\n{}", i.name, i.description),
            calendar: i.calendar,
            computed: false,
        };
        columns.extend(uaq.into_iter().map(builtin));
        columns.extend(computed.iter().map(|(spec, cal)| Column {
            id: spec.id(),
            label: spec.short_label().to_owned(),
            hover: format!("{}\n{}", spec.name(), spec.description()),
            calendar: cal.as_ref(),
            computed: true,
        }));
        columns.extend(tabular.into_iter().map(builtin));
        let cals: Vec<&dyn HijriCalendar> = columns.iter().map(|c| c.calendar).collect();
        let rows = compare_month_starts(year, &cals);
        let current = &tc.settings.calendar.calendar_id;
        let warn = ui.visuals().warn_fg_color;
        let computing = self.job.is_running();

        egui::ScrollArea::horizontal()
            .id_salt("comparison_scroll")
            .show(ui, |ui| {
                egui::Grid::new("comparison")
                    .striped(true)
                    .spacing([10.0, 4.0])
                    .show(ui, |ui| {
                        ui.strong("Month");
                        for column in &columns {
                            let label = egui::RichText::new(&column.label);
                            let label = if column.id == current {
                                label.strong().underline()
                            } else {
                                label.strong()
                            };
                            ui.label(label).on_hover_text(&column.hover);
                        }
                        ui.end_row();

                        for row in &rows {
                            let name = hijri_month_name(row.month, diacritics);
                            if row.disagrees() {
                                ui.colored_label(warn, format!("{name} ≠"))
                                    .on_hover_text("The calendars disagree on this month's start");
                            } else {
                                ui.label(name);
                            }
                            for (i, start) in row.starts.iter().enumerate() {
                                match start {
                                    None if columns[i].computed && computing => {
                                        ui.weak("…").on_hover_text("Computing…");
                                    }
                                    None => {
                                        ui.weak("—").on_hover_text("Outside this calendar's range");
                                    }
                                    Some(jdn) => {
                                        let civil = CivilDate::from_jdn(*jdn, CIVIL);
                                        let text = egui::RichText::new(format_civil_short(civil))
                                            .monospace();
                                        let text = if row.differs(i) {
                                            text.color(warn).strong()
                                        } else {
                                            text
                                        };
                                        let tip = format!(
                                            "1 {} {year} AH = {} {}",
                                            name,
                                            Weekday::from_jdn(*jdn).name_english(),
                                            format_civil(civil)
                                        );
                                        if ui
                                            .add(egui::Label::new(text).sense(egui::Sense::click()))
                                            .on_hover_text(format!("{tip}\nClick to select"))
                                            .clicked()
                                        {
                                            set_day(&mut tc.settings.time, *jdn);
                                        }
                                    }
                                }
                            }
                            ui.end_row();
                        }
                    });
            });
        ui.weak(
            "Highlighted dates differ from the most common start among the calendars. \
             The underlined column is the selected calendar.",
        );
    }
}

/// The calendar selector, with the selected calendar's description and
/// data attribution.
fn calendar_selector(ui: &mut egui::Ui, settings: &mut Settings) {
    let specs = computed_specs(settings);
    let cs = &mut settings.calendar;
    let spec = specs.iter().find(|s| s.id() == cs.calendar_id).cloned();
    let info = selected_info(&cs.calendar_id);
    if spec.is_none() && info.id != cs.calendar_id {
        cs.calendar_id = info.id.to_owned();
    }
    let selected_name = spec
        .as_ref()
        .map_or(info.name.to_owned(), ComputedSpec::name);
    ui.horizontal(|ui| {
        ui.label("Calendar:");
        egui::ComboBox::from_id_salt("hijri_calendar")
            .selected_text(selected_name)
            .width(320.0)
            .show_ui(ui, |ui| {
                for c in calendars() {
                    ui.selectable_value(&mut cs.calendar_id, c.id.to_owned(), c.name)
                        .on_hover_text(c.description);
                }
                ui.separator();
                ui.weak("Computed with the ephemeris:");
                for s in &specs {
                    let label = match s {
                        ComputedSpec::Visibility { .. } => "Visibility-based (crescent sighting)",
                        ComputedSpec::Rule(r) => r.name(),
                    };
                    ui.selectable_value(&mut cs.calendar_id, s.id().to_owned(), label)
                        .on_hover_text(s.description());
                }
            });
    });
    match specs.iter().find(|s| s.id() == cs.calendar_id) {
        Some(spec) => {
            ui.add(egui::Label::new(spec.description()).wrap());
            match spec {
                ComputedSpec::Visibility { .. } => {
                    visibility_settings(ui, &mut cs.visibility, &settings.location);
                }
                ComputedSpec::Rule(r) => {
                    ui.add(
                        egui::Label::new(egui::RichText::new(r.reference()).small().weak()).wrap(),
                    );
                }
            }
        }
        None => {
            let info = selected_info(&cs.calendar_id);
            ui.add(egui::Label::new(info.description).wrap());
            if let Some(credit) = info.attribution {
                ui.add(egui::Label::new(egui::RichText::new(credit).small().weak()).wrap());
            }
        }
    }
}

/// The settings of the visibility-based calendar: criterion, threshold
/// zone, and where the crescent is looked for.
fn visibility_settings(
    ui: &mut egui::Ui,
    vs: &mut VisibilityCalendarSettings,
    location: &LocationSettings,
) {
    let criteria = mooncalc_visibility::criteria::all();
    if !criteria.iter().any(|c| c.id() == vs.criterion_id)
        && let Some(first) = criteria.first()
    {
        vs.criterion_id = first.id().to_owned();
    }
    ui.horizontal_wrapped(|ui| {
        ui.label("Criterion:");
        let current = criteria.iter().find(|c| c.id() == vs.criterion_id);
        egui::ComboBox::from_id_salt("hijri_visibility_criterion")
            .selected_text(current.map_or("—", |c| c.name()))
            .show_ui(ui, |ui| {
                for c in &criteria {
                    ui.selectable_value(&mut vs.criterion_id, c.id().to_owned(), c.name())
                        .on_hover_text(c.reference());
                }
            });
        if let Some(c) = criteria.iter().find(|c| c.id() == vs.criterion_id) {
            // Every zone but the last ("not visible") can be the threshold.
            let zones = c.zones();
            let choices = zones.len().saturating_sub(1).max(1);
            vs.max_zone = vs.max_zone.min(choices - 1);
            ui.label("Seen at:");
            egui::ComboBox::from_id_salt("hijri_visibility_zone")
                .selected_text(format!("zone {} or better", zones[vs.max_zone].code))
                .show_ui(ui, |ui| {
                    for (i, z) in zones.iter().take(choices).enumerate() {
                        ui.selectable_value(
                            &mut vs.max_zone,
                            i,
                            format!("{}: {}", z.code, z.description),
                        );
                    }
                });
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("Where:");
        let mut options = vec![
            (
                OBSERVERS_APP_LOCATION.to_owned(),
                format!("Observer location ({})", location.name),
            ),
            (OBSERVERS_MECCA.to_owned(), "Mecca".to_owned()),
        ];
        options.extend(
            RegionPreset::ALL
                .iter()
                .map(|r| (r.id().to_owned(), format!("Anywhere: {}", r.name()))),
        );
        if !options.iter().any(|(id, _)| *id == vs.observers) {
            vs.observers = OBSERVERS_APP_LOCATION.to_owned();
        }
        let current = options
            .iter()
            .find(|(id, _)| *id == vs.observers)
            .map_or(String::new(), |(_, name)| name.clone());
        egui::ComboBox::from_id_salt("hijri_visibility_where")
            .selected_text(current)
            .show_ui(ui, |ui| {
                for (id, name) in &options {
                    ui.selectable_value(&mut vs.observers, id.clone(), name);
                }
            });
        if RegionPreset::from_id(&vs.observers).is_some() {
            ui.label("Grid:");
            ui.add(
                egui::DragValue::new(&mut vs.grid_step_deg)
                    .range(2.0..=20.0)
                    .speed(0.1)
                    .max_decimals(1)
                    .suffix("°"),
            )
            .on_hover_text(
                "Spacing of the sample points in the region. Each point is a sea-level \
                 observer on its own local evening; the month starts when the crescent is \
                 seen at any of them. Finer grids are slower.",
            );
        }
    });
}

/// Draw a month grid. Returns the day clicked, if any.
fn month_grid(
    ui: &mut egui::Ui,
    grid: &MonthGrid,
    view: MonthView,
    week_start: Weekday,
    selected: i64,
    today: i64,
    diacritics: bool,
) -> Option<i64> {
    let gap = 4.0;
    let width = ((ui.available_width() - 6.0 * gap) / 7.0).clamp(52.0, 120.0);
    let size = egui::vec2(width, 46.0);
    let mut clicked = None;

    egui::Grid::new("month_grid")
        .spacing([gap, gap])
        .min_col_width(width)
        .show(ui, |ui| {
            for i in 0..7u8 {
                let wd = Weekday::ALL[usize::from((week_start.index_from_sunday() + i) % 7)];
                ui.vertical_centered(|ui| {
                    ui.strong(&wd.name_english()[..3]);
                });
            }
            ui.end_row();
            for week in grid.weeks(week_start) {
                for cell in week {
                    match cell {
                        None => {
                            ui.allocate_exact_size(size, egui::Sense::hover());
                        }
                        Some(day) => {
                            if day_cell(ui, size, &day, view, selected, today, diacritics) {
                                clicked = Some(day.jdn);
                            }
                        }
                    }
                }
                ui.end_row();
            }
        });
    clicked
}

/// One day of the month view. Returns `true` when clicked.
fn day_cell(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    day: &GridDay,
    view: MonthView,
    selected: i64,
    today: i64,
    diacritics: bool,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let visuals = ui.visuals();
    let is_selected = day.jdn == selected;
    let fill = if is_selected {
        visuals.selection.bg_fill
    } else if response.hovered() {
        visuals.widgets.hovered.weak_bg_fill
    } else {
        visuals.faint_bg_color
    };
    let text_color = if is_selected {
        visuals.selection.stroke.color
    } else {
        visuals.text_color()
    };
    let weak_color = if is_selected {
        visuals.selection.stroke.color
    } else {
        visuals.weak_text_color()
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, fill);
    if day.jdn == today {
        painter.rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(2.0, visuals.warn_fg_color),
            egui::StrokeKind::Inside,
        );
    }

    let (primary, secondary) = match view {
        MonthView::Hijri { .. } => {
            let primary = day.hijri.map_or("?".to_owned(), |h| h.day.to_string());
            let c = day.civil;
            let secondary = format!("{} {}", c.day, &CIVIL_MONTHS[usize::from(c.month - 1)][..3]);
            (primary, secondary)
        }
        MonthView::Civil { .. } => {
            let primary = day.civil.day.to_string();
            let secondary = match day.hijri {
                Some(h) => format!("{} {}", h.day, HIJRI_MONTH_ABBR[usize::from(h.month - 1)]),
                None => "—".to_owned(),
            };
            (primary, secondary)
        }
    };
    let pad = egui::vec2(6.0, 3.0);
    painter.text(
        rect.left_top() + pad,
        egui::Align2::LEFT_TOP,
        primary,
        egui::FontId::proportional(18.0),
        text_color,
    );
    painter.text(
        rect.right_bottom() - pad,
        egui::Align2::RIGHT_BOTTOM,
        secondary,
        egui::FontId::proportional(11.0),
        weak_color,
    );

    let hijri = day
        .hijri
        .map_or("outside the calendar's range".to_owned(), |h| {
            format_hijri(h, diacritics)
        });
    let mut tip = format!(
        "{} {}\n{hijri}",
        day.weekday.name_english(),
        format_civil(day.civil)
    );
    if day.jdn == today {
        tip.push_str("\nToday");
    }
    tip.push_str("\nClick to select this date");
    response.on_hover_text(tip).clicked()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mooncalc_hijri::UMM_AL_QURA;

    fn jdn(y: i32, m: u8, d: u8) -> i64 {
        CivilDate::new(y, m, d).to_jdn(CIVIL).unwrap()
    }

    #[test]
    fn unknown_calendar_falls_back_to_default() {
        assert_eq!(selected_info("nope").id, DEFAULT_CALENDAR_ID);
        assert_eq!(selected_info("tabular-2-civil").id, "tabular-2-civil");
        assert_eq!(CalendarSettings::default().calendar_id, DEFAULT_CALENDAR_ID);
    }

    #[test]
    fn settings_round_trip() {
        let s = CalendarSettings {
            calendar_id: "tabular-3-civil".into(),
            primary: PrimaryCalendar::Gregorian,
            week_start: WeekStart::Monday,
            visibility: VisibilityCalendarSettings {
                criterion_id: "odeh".into(),
                max_zone: 2,
                observers: "americas".into(),
                grid_step_deg: 4.0,
            },
        };
        let back: CalendarSettings =
            serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
        let partial: CalendarSettings = serde_json::from_str(r#"{"primary":"Gregorian"}"#).unwrap();
        assert_eq!(partial.calendar_id, DEFAULT_CALENDAR_ID);
        assert_eq!(partial.primary, PrimaryCalendar::Gregorian);
        assert_eq!(partial.visibility, VisibilityCalendarSettings::default());
    }

    #[test]
    fn computed_specs_and_keys() {
        let mut settings = Settings::default();
        let specs = computed_specs(&settings);
        assert_eq!(specs.len(), 1 + RuleCalendar::ALL.len());
        assert!(specs.iter().all(|s| is_computed_id(s.id())));
        assert!(!is_computed_id(DEFAULT_CALENDAR_ID));
        let v = &specs[0];
        assert_eq!(v.id(), VISIBILITY_ID);
        assert!(v.name().contains("≤ B"), "{}", v.name());
        let key = v.key(settings.backend, "");
        // Moving the observer changes the visibility calendar, not the rules.
        settings.location.lat_deg += 1.0;
        let moved = computed_specs(&settings);
        assert_ne!(moved[0].key(settings.backend, ""), key);
        assert_eq!(
            moved[1].key(settings.backend, ""),
            specs[1].key(settings.backend, "")
        );
        // A region ignores the observer location.
        settings.calendar.visibility.observers = RegionPreset::AsiaPacific.id().into();
        let region = ComputedSpec::visibility(&settings.calendar.visibility, &settings.location);
        assert!(matches!(
            region,
            ComputedSpec::Visibility {
                observers: Observers::Region { .. },
                ..
            }
        ));
        assert!(region.name().contains("Asia"));
        // An unknown observer id falls back to the observer location.
        settings.calendar.visibility.observers = "atlantis".into();
        let back = ComputedSpec::visibility(&settings.calendar.visibility, &settings.location);
        assert!(matches!(
            back,
            ComputedSpec::Visibility {
                observers: Observers::Place(_),
                ..
            }
        ));
    }

    #[test]
    fn computes_a_year_in_the_background() {
        // The job function on the calling thread: the Umm al-Qura rule for
        // 1445 AH, then shown through a `KnownStarts` calendar.
        let engine = Engine::default();
        let spec = ComputedSpec::Rule(RuleCalendar::UmmAlQuraRule);
        let req = YearRequest {
            key: spec.key(BackendChoice::Meeus, ""),
            spec: spec.clone(),
            year: 1445,
        };
        let out = compute_batch(
            &engine,
            BackendChoice::Meeus,
            "",
            std::slice::from_ref(&req),
            &JobContext::detached(),
        )
        .unwrap();
        assert_eq!(out.len(), 1);
        let (key, year, starts) = out.into_iter().next().unwrap();
        assert_eq!((key.as_str(), year), (req.key.as_str(), 1445));
        assert_eq!(starts.len(), 13);
        let mut c = Computed::default();
        c.add(year, starts, &spec);
        let cal = c.calendar.unwrap();
        assert_eq!(cal.supported_range().first_year, 1445);
        // 1 Ramaḍān 1445 = 11 March 2024 under both the rule and the table.
        assert_eq!(cal.month_start(1445, 9), Some(jdn(2024, 3, 11)));
        assert_eq!(
            cal.to_hijri(jdn(2024, 3, 20)),
            Some(HijriDate::new(1445, 9, 10))
        );
        assert_eq!(cal.year_length(1445), UMM_AL_QURA.year_length(1445));
        assert_eq!(cal.to_hijri(jdn(2030, 1, 1)), None);
        // The provider is cached for the next job.
        assert_eq!(engine.lock_providers().len(), 1);
    }

    /// Meeus claiming coverage of 2022-06-01 to 2024-03-31 only (TT): from
    /// before 1 Muharram 1444, which the chain of 1445 AH starts from.
    struct HalfYear;

    impl HalfYear {
        fn ok(&self, t: f64) -> bool {
            let (a, b) = self.valid_range();
            (a..=b).contains(&t)
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
            if self.ok(t) { e } else { nan_equatorial() }
        }
        fn moon_equatorial(&self, t: f64) -> mooncalc_core::Equatorial {
            let e = mooncalc_core::MeeusEphemeris::new().moon_equatorial(t);
            if self.ok(t) { e } else { nan_equatorial() }
        }
        fn sun_ecliptic(&self, t: f64) -> mooncalc_core::Ecliptic {
            let e = mooncalc_core::MeeusEphemeris::new().sun_ecliptic(t);
            if self.ok(t) { e } else { nan_ecliptic() }
        }
        fn moon_ecliptic(&self, t: f64) -> mooncalc_core::Ecliptic {
            let e = mooncalc_core::MeeusEphemeris::new().moon_ecliptic(t);
            if self.ok(t) { e } else { nan_ecliptic() }
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

    #[test]
    fn partly_covered_years_keep_only_covered_months() {
        let eph: Arc<dyn Ephemeris> = Arc::new(HalfYear);
        // 1445 AH runs from July 2023 to July 2024: partly covered. 1446 AH
        // (from July 2024) is outside.
        assert_eq!(
            hijri_year_coverage(eph.as_ref(), 1445),
            YearCoverage::Partial
        );
        assert_eq!(
            hijri_year_coverage(eph.as_ref(), 1446),
            YearCoverage::Outside
        );
        let active = backend::ActiveEphemeris {
            eph: eph.clone(),
            used: BackendChoice::De440,
            notice: None,
        };
        let note = coverage_note(&active, &[1446, 1445]).unwrap();
        assert!(note.contains("year 1445 only partly inside"), "{note}");
        assert!(note.contains("year 1446 outside"), "{note}");
        assert!(coverage_note(&active, &[]).is_none());

        // The Umm al-Qura rule over the covered months agrees with the
        // table; the months outside are left out, not lengthened.
        let spec = ComputedSpec::Rule(RuleCalendar::UmmAlQuraRule);
        let provider = spec.provider(eph.clone()).unwrap();
        let first = month_index(1445, 1);
        let mut kept = Vec::new();
        for n in first..=first + 12 {
            let (y, m) = month_from_index(n);
            let estimate = TABULAR_II_CIVIL.month_start(y, m);
            if estimate.is_none_or(|e| estimate_covered(eph.as_ref(), e))
                && let Some(jdn) = provider.month_start(y, m)
                && start_covered(eph.as_ref(), jdn)
            {
                assert_eq!(UMM_AL_QURA.month_start(y, m), Some(jdn), "{y}-{m}");
                kept.push(m);
            }
        }
        // Muharram (2023-07-19) to Ramadan (2024-03-11) start inside;
        // Shawwal (2024-04-10) does not.
        assert_eq!(kept, (1..=9).collect::<Vec<u8>>(), "{kept:?}");
    }

    #[test]
    fn active_calendar_switches_to_computed() {
        let mut tab = CalendarTab::default();
        let mut settings = Settings::default();
        assert!(matches!(
            tab.active_calendar(&settings),
            ActiveCalendar::Builtin(_)
        ));
        settings.calendar.calendar_id = RuleCalendar::ConjunctionBeforeNoonUt.id().into();
        let empty = tab.active_calendar(&settings);
        let ActiveCalendar::Computed(cal) = &empty else {
            panic!("expected a computed calendar")
        };
        assert_eq!(cal.to_hijri(jdn(2024, 3, 20)), None);
        // Requests: every computed calendar for the years on screen, the
        // selected calendar first.
        tab.wanted_years = vec![1445, 1446];
        let missing = tab.missing(&settings);
        assert_eq!(missing.len(), 2 * computed_specs(&settings).len());
        assert_eq!(
            missing[0].spec.id(),
            RuleCalendar::ConjunctionBeforeNoonUt.id()
        );
        assert_eq!(missing[1].year, 1446);
        // Once a year is in, it is no longer requested and is shown.
        let spec = missing[0].spec.clone();
        let starts = (0..13)
            .map(|i| (month_index(1445, 1) + i, jdn(2023, 7, 19) + 29 * i + i / 2))
            .collect();
        tab.computed
            .entry(missing[0].key.clone())
            .or_default()
            .add(1445, starts, &spec);
        assert_eq!(tab.missing(&settings).len(), missing.len() - 1);
        let ActiveCalendar::Computed(cal) = tab.active_calendar(&settings) else {
            panic!()
        };
        assert_eq!(cal.month_start(1445, 1), Some(jdn(2023, 7, 19)));
    }

    #[test]
    fn set_day_keeps_local_time_of_day() {
        // 2024-03-10 22:30 UT = 2024-03-11 01:30 at UTC+3.
        let mut t = TimeSettings {
            jd_ut: datetime::calendar_to_jd(2024, 3, 10.0 + 22.5 / 24.0),
            tz_offset_hours: 3.0,
            edit_local: true,
            start_at_now: false,
            ..TimeSettings::default()
        };
        assert_eq!(selected_day(&t), jdn(2024, 3, 11));
        set_day(&mut t, jdn(2024, 4, 9));
        assert_eq!(selected_day(&t), jdn(2024, 4, 9));
        let expected = datetime::calendar_to_jd(2024, 4, 8.0 + 22.5 / 24.0);
        assert!(
            (t.jd_ut - expected).abs() < 1e-9,
            "{} vs {expected}",
            t.jd_ut
        );
    }

    #[test]
    fn month_view_contains_and_steps() {
        let cal: &dyn HijriCalendar = &UMM_AL_QURA;
        let day = jdn(2024, 3, 11); // 1 Ramadan 1445 (Umm al-Qura)
        let v = MonthView::containing(day, PrimaryCalendar::Hijri, cal);
        assert_eq!(
            v,
            MonthView::Hijri {
                year: 1445,
                month: 9
            }
        );
        assert_eq!(
            v.step(4),
            MonthView::Hijri {
                year: 1446,
                month: 1
            }
        );
        assert_eq!(
            v.step(-9),
            MonthView::Hijri {
                year: 1444,
                month: 12
            }
        );
        assert_eq!(v.grid(cal).unwrap().days[0].jdn, day);
        assert_eq!(v.title(false), "Ramadan 1445 AH");

        let c = MonthView::containing(day, PrimaryCalendar::Gregorian, cal);
        assert_eq!(
            c,
            MonthView::Civil {
                year: 2024,
                month: 3
            }
        );
        assert_eq!(
            c.step(-3),
            MonthView::Civil {
                year: 2023,
                month: 12
            }
        );
        assert_eq!(c.grid(cal).unwrap().days.len(), 31);
        assert_eq!(c.title(false), "March 2024");

        // Outside Umm al-Qura's range the Hijri view clamps to its ends.
        let early = MonthView::containing(jdn(1800, 1, 1), PrimaryCalendar::Hijri, cal);
        let r = cal.supported_range();
        assert_eq!(
            early,
            MonthView::Hijri {
                year: r.first_year,
                month: 1
            }
        );
        let late = MonthView::containing(jdn(2200, 1, 1), PrimaryCalendar::Hijri, cal);
        assert_eq!(
            late,
            MonthView::Hijri {
                year: r.last_year,
                month: 12
            }
        );
        assert!(late.step(1).grid(cal).is_none());
    }

    #[test]
    fn comparison_finds_consensus_and_disagreements() {
        let infos = calendars();
        let cals: Vec<&dyn HijriCalendar> = infos.iter().map(|i| i.calendar as _).collect();
        let rows = compare_month_starts(1445, &cals);
        assert_eq!(rows.len(), 12);
        let uq = infos.iter().position(|i| i.id == "umm-al-qura").unwrap();
        let ramadan = &rows[8];
        assert_eq!(ramadan.month, 9);
        assert_eq!(ramadan.starts.len(), infos.len());
        assert_eq!(ramadan.starts[uq], Some(jdn(2024, 3, 11)));
        // Each row's consensus is one of its starts, and every calendar
        // covers 1445 AH.
        for row in &rows {
            assert!(row.starts.iter().all(Option::is_some));
            assert!(row.starts.contains(&row.consensus));
            assert_eq!(
                row.disagrees(),
                (0..row.starts.len()).any(|i| row.differs(i))
            );
        }
        // Civil and astronomical epochs differ by one day, so some month
        // must disagree.
        assert!(rows.iter().any(ComparisonRow::disagrees));

        // Hand-made case: two calendars agree, one is a day late.
        let a: &dyn HijriCalendar = &UMM_AL_QURA;
        let rows = compare_month_starts(1445, &[a, a, infos[2].calendar]);
        for row in rows {
            assert_eq!(row.consensus, row.starts[0]);
            assert!(!row.differs(0) && !row.differs(1));
            assert_eq!(row.differs(2), row.starts[2] != row.starts[0]);
        }

        // Out of range: Umm al-Qura has no 1600 AH; the others still agree
        // among themselves.
        let rows = compare_month_starts(1600, &cals);
        assert!(rows[0].starts[uq].is_none());
        assert!(!rows[0].differs(uq));
    }

    #[test]
    fn formats_dates() {
        assert_eq!(
            format_civil(CivilDate::new(2026, 11, 11)),
            "11 November 2026"
        );
        assert_eq!(format_civil_short(CivilDate::new(2026, 1, 2)), "2 Jan 2026");
        assert_eq!(
            format_hijri(HijriDate::new(1445, 9, 1), false),
            "1 Ramadan 1445 AH"
        );
        assert_eq!(
            format_hijri(HijriDate::new(1445, 9, 1), true),
            "1 Ramaḍān 1445 AH"
        );
        assert_eq!(short_label("tabular-2-civil"), "II civil");
    }
}
