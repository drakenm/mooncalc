//! Wire types of the [`crate::calendar`] service. Same conventions as
//! [`crate::dto`] (camelCase JSON, one `params` object per operation). The
//! TypeScript mirror is `frontend/tauri/src/backend/types-calendar.ts`.
//!
//! Civil days travel both as a [`CivilDate`] (Julian before 15 October
//! 1582, Gregorian from then on) and as a Julian Day Number (`jdn`, the
//! Julian Date at noon of that civil day), which is what the calendars
//! count in.

use serde::{Deserialize, Serialize};

use crate::dto::{CivilDate, Location};

/// [`VisibilityCalendarSpec::observers`]: the observer location given in
/// [`VisibilityCalendarSpec::location`].
pub const OBSERVERS_LOCATION: &str = "location";
/// [`VisibilityCalendarSpec::observers`]: the Kaʿba, Mecca.
pub const OBSERVERS_MECCA: &str = "mecca";

/// Which calendar to use.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarSpec {
    /// A [`CalendarDescription::id`]: a built-in calendar (`umm-al-qura`,
    /// `tabular-2-civil` ...), a rule calendar (`rule-umm-al-qura` ...) or
    /// `visibility`.
    pub id: String,
    /// Settings of the visibility-based calendar; required when `id` is
    /// `visibility`, ignored otherwise.
    #[serde(default)]
    pub visibility: Option<VisibilityCalendarSpec>,
}

/// Settings of the visibility-based calendar: a month starts the day after
/// the evening of its 29th day if the crescent is seen then, otherwise it
/// completes 30 days.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisibilityCalendarSpec {
    /// `CriterionInfo::id`.
    pub criterion_id: String,
    /// Threshold zone: seen in this zone or a better one (0 = the
    /// criterion's most visible zone).
    pub max_zone: u8,
    /// [`OBSERVERS_LOCATION`], [`OBSERVERS_MECCA`] or a
    /// [`RegionDescription::id`] ("seen anywhere in the region").
    pub observers: String,
    /// The observer for [`OBSERVERS_LOCATION`].
    #[serde(default)]
    pub location: Option<Location>,
    /// Display name of that location.
    #[serde(default)]
    pub location_name: Option<String>,
    /// Grid spacing for regions, degrees (default 5, clamped to 1..=30).
    #[serde(default)]
    pub grid_step_deg: Option<f64>,
}

// ---------------------------------------------------------------------------
// calendar_list
// ---------------------------------------------------------------------------

/// Input of `calendar_list`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarListParams {
    /// Settings of the visibility-based calendar, to name it (default:
    /// Yallop zone B or better at Mecca).
    #[serde(default)]
    pub visibility: Option<VisibilityCalendarSpec>,
}

/// The family of a calendar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CalendarFamily {
    /// Table-driven Umm al-Qura.
    UmmAlQura,
    /// Arithmetical 30-year cycle.
    Tabular,
    /// An astronomical rule computed with the ephemeris.
    Rule,
    /// Crescent visibility computed with the ephemeris.
    Visibility,
}

/// One selectable calendar.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarDescription {
    pub id: String,
    pub name: String,
    /// A short column label, e.g. "UQ (ICU)".
    pub short_label: String,
    pub description: String,
    pub family: CalendarFamily,
    /// Whether the month starts are computed with the ephemeris (rule and
    /// visibility calendars), and so must be computed per Hijri year.
    pub computed: bool,
    /// CLDR/ICU calendar id (`Intl.DateTimeFormat`), if any.
    pub cldr_id: Option<String>,
    /// Text that must be shown with the calendar's data.
    pub attribution: Option<String>,
    /// Bibliographic source of a rule.
    pub reference: Option<String>,
    /// The Hijri years covered, for table-driven and arithmetical calendars.
    pub first_year: Option<i32>,
    pub last_year: Option<i32>,
}

/// A region preset of the visibility-based calendar.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionDescription {
    pub id: String,
    pub name: String,
}

/// The names of a Hijri month.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HijriMonthName {
    /// 1..=12.
    pub number: u8,
    /// Transliteration with diacritics, e.g. "Ramaḍān".
    pub name: String,
    /// Plain ASCII, e.g. "Ramadan".
    pub ascii: String,
    /// Arabic script, e.g. "رمضان".
    pub arabic: String,
}

/// The names of a weekday.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekdayName {
    /// 0 = Sunday ... 6 = Saturday.
    pub index: u8,
    pub english: String,
    /// e.g. "al-Jumuʿah".
    pub transliterated: String,
    /// e.g. "الجمعة".
    pub arabic: String,
}

/// Result of `calendar_list`: the calendars and the names a calendar view
/// needs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarCatalog {
    /// Built-in Umm al-Qura tables, the computed calendars (visibility,
    /// rules), then the arithmetical calendars.
    pub calendars: Vec<CalendarDescription>,
    /// The id to select by default.
    pub default_id: String,
    pub regions: Vec<RegionDescription>,
    /// Muḥarram first.
    pub hijri_months: Vec<HijriMonthName>,
    /// Sunday first.
    pub weekdays: Vec<WeekdayName>,
    /// "The Islamic day begins at sunset ..." note for calendar views.
    pub day_convention: String,
}

// ---------------------------------------------------------------------------
// Shared value types
// ---------------------------------------------------------------------------

/// A Hijri date.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HijriYmd {
    pub year: i32,
    /// 1..=12.
    pub month: u8,
    /// 1..=30.
    pub day: u8,
}

/// One civil day with its dates.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarDay {
    pub jdn: i64,
    pub civil: CivilDate,
    /// `None` outside the calendar's range (or not computed yet).
    pub hijri: Option<HijriYmd>,
    /// 0 = Sunday ... 6 = Saturday.
    pub weekday: u8,
}

/// A computed (calendar, Hijri year) a request needs but that has not been
/// computed yet. Compute it with `calendar_month_starts`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingYear {
    pub calendar_id: String,
    pub hijri_year: i32,
}

// ---------------------------------------------------------------------------
// calendar_month_starts
// ---------------------------------------------------------------------------

/// Input of `calendar_month_starts`: compute one Hijri year of a computed
/// calendar (the expensive step, meant for a worker thread).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthStartsParams {
    pub calendar: CalendarSpec,
    pub hijri_year: i32,
    #[serde(default)]
    pub ephemeris_id: Option<String>,
}

/// The first civil day of a Hijri month.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthStart {
    pub year: i32,
    pub month: u8,
    pub jdn: i64,
    pub civil: CivilDate,
}

/// Result of `calendar_month_starts`: the starts of months 1..=12 of the
/// year and of the following Muḥarram (which fixes the length of Dhū
/// al-Ḥijjah).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthStartsYear {
    pub calendar_id: String,
    pub calendar_name: String,
    pub hijri_year: i32,
    pub starts: Vec<MonthStart>,
}

// ---------------------------------------------------------------------------
// calendar_month
// ---------------------------------------------------------------------------

/// Which calendar a month view is laid out by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrimaryCalendar {
    /// A Hijri month with its civil dates.
    #[default]
    Hijri,
    /// A civil month with its Hijri dates.
    Gregorian,
}

/// A month of the primary calendar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthRef {
    pub year: i32,
    /// 1..=12.
    pub month: u8,
}

/// Input of `calendar_month`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarMonthParams {
    pub calendar: CalendarSpec,
    #[serde(default)]
    pub primary: PrimaryCalendar,
    /// The month to show (of the primary calendar). When absent, the month
    /// containing `date`.
    #[serde(default)]
    pub month: Option<MonthRef>,
    /// A civil date whose month to show when `month` is absent.
    #[serde(default)]
    pub date: Option<CivilDate>,
    /// First column, 0 = Sunday ... 6 = Saturday (default 6, Saturday).
    #[serde(default)]
    pub week_start: Option<u8>,
    #[serde(default)]
    pub ephemeris_id: Option<String>,
}

/// A month laid out in weeks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarMonth {
    pub calendar_id: String,
    pub calendar_name: String,
    pub primary: PrimaryCalendar,
    /// The month shown.
    pub month: MonthRef,
    pub prev: MonthRef,
    pub next: MonthRef,
    /// The weekday of each column, 0 = Sunday.
    pub columns: Vec<u8>,
    /// Rows of seven cells; `None` before the first and after the last day.
    /// Empty when the month is outside the calendar's range.
    pub weeks: Vec<Vec<Option<CalendarDay>>>,
    /// The months of the other calendar this month touches, in order
    /// (Hijri months for a Gregorian view and vice versa).
    pub other_months: Vec<MonthRef>,
    /// Why there are no days, when there are none.
    pub message: Option<String>,
    /// Computed years needed but not yet computed; the view is incomplete
    /// until they are.
    pub missing: Vec<MissingYear>,
}

// ---------------------------------------------------------------------------
// calendar_convert
// ---------------------------------------------------------------------------

/// Input of `calendar_convert`: give exactly one of `civil`, `hijri`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarConvertParams {
    pub calendar: CalendarSpec,
    #[serde(default)]
    pub civil: Option<CivilDate>,
    #[serde(default)]
    pub hijri: Option<HijriYmd>,
    #[serde(default)]
    pub ephemeris_id: Option<String>,
}

/// Result of `calendar_convert`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarConversion {
    /// The day, when the input date exists.
    pub day: Option<CalendarDay>,
    /// The length of the Hijri month of the result (or of the input Hijri
    /// month), when known.
    pub month_length: Option<u8>,
    /// Why there is no result (no such date, outside the range ...).
    pub message: Option<String>,
    pub missing: Vec<MissingYear>,
}

// ---------------------------------------------------------------------------
// calendar_compare
// ---------------------------------------------------------------------------

/// Input of `calendar_compare`: the start of every month of a Hijri year
/// under every calendar.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarCompareParams {
    pub hijri_year: i32,
    /// Settings of the visibility-based calendar's column (omit to leave it
    /// out).
    #[serde(default)]
    pub visibility: Option<VisibilityCalendarSpec>,
    /// Leave out the computed (rule and visibility) columns.
    #[serde(default)]
    pub builtin_only: bool,
    #[serde(default)]
    pub ephemeris_id: Option<String>,
}

/// One column of the comparison.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareColumn {
    pub id: String,
    pub label: String,
    pub name: String,
    pub description: String,
    pub computed: bool,
}

/// The starts of one month under every calendar.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareRow {
    pub month: u8,
    /// Per column; `None` where the calendar does not cover the month (or
    /// has not computed it yet).
    pub starts: Vec<Option<MonthStart>>,
    /// The most common start (ties go to the earlier column).
    pub consensus: Option<i64>,
    /// Whether the calendars that cover the month disagree.
    pub disagrees: bool,
    /// Per column: whether it differs from the consensus.
    pub differs: Vec<bool>,
}

/// Result of `calendar_compare`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarComparison {
    pub hijri_year: i32,
    pub columns: Vec<CompareColumn>,
    pub rows: Vec<CompareRow>,
    pub missing: Vec<MissingYear>,
}
