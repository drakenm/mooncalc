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
//! **Extension point (visibility-based calendars):** month starts predicted
//! from crescent visibility at the observer's location come from
//! [`mooncalc_hijri::ObservedCalendar`] with a
//! [`mooncalc_hijri::MonthStartProvider`] backed by the visibility engine
//! (`settings.location`, `settings.criterion_id`). To add it: extend
//! [`active_calendar`] to return such a calendar for a new id (the selector
//! shows a disabled "Visibility-based (coming soon)" entry where it goes),
//! and run its month-start scans as jobs ([`crate::jobs`]) from a `poll`
//! method, since each month start needs a visibility search. Everything
//! else in this tab works on `&dyn HijriCalendar`.

use mooncalc_hijri::civil::{CivilCalendar, CivilDate};
use mooncalc_hijri::{
    CalendarInfo, DEFAULT_CALENDAR_ID, GridDay, HijriCalendar, HijriDate, HijriMonth, MonthGrid,
    Weekday, calendars, civil_month_grid, hijri_month_grid, info_by_id,
};
use serde::{Deserialize, Serialize};

use crate::datetime;
use crate::settings::TimeSettings;

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
}

impl Default for CalendarSettings {
    fn default() -> Self {
        Self {
            calendar_id: DEFAULT_CALENDAR_ID.into(),
            primary: PrimaryCalendar::Hijri,
            week_start: WeekStart::Saturday,
        }
    }
}

/// The registry entry for `id`, or the default calendar for an unknown id.
pub fn selected_info(id: &str) -> &'static CalendarInfo {
    info_by_id(id)
        .or_else(|| info_by_id(DEFAULT_CALENDAR_ID))
        .unwrap_or(&calendars()[0])
}

/// The calendar the tab works with. Today a built-in calendar; the hook for
/// visibility-based calendars (see the module docs).
pub fn active_calendar(settings: &CalendarSettings) -> &'static (dyn HijriCalendar + Sync) {
    selected_info(&settings.calendar_id).calendar
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
    civil_day(datetime::now_jd() + time.tz_offset_hours / 24.0)
}

/// Move `time` to local civil day `jdn`, keeping the local time of day.
pub fn set_day(time: &mut TimeSettings, jdn: i64) {
    let local = time.jd_local();
    let fraction = local + 0.5 - (local + 0.5).floor();
    time.jd_ut = jdn as f64 - 0.5 + fraction - time.tz_offset_hours / 24.0;
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

/// Converter inputs.
#[derive(Debug, Clone, Copy)]
struct ConverterState {
    civil: (i32, u8, u8),
    hijri: (i32, u8, u8),
}

/// State of the Calendar tab.
#[derive(Debug, Default)]
pub struct CalendarTab {
    /// The month on screen; `None` until first drawn.
    view: Option<MonthView>,
    /// What the view was last centred on: (selected day, primary, calendar).
    centred_on: Option<(i64, PrimaryCalendar, String)>,
    converter: Option<ConverterState>,
    /// Hijri year of the comparison table; `None` follows the selected date.
    comparison_year: Option<i32>,
    /// Whether the fonts can show the transliteration diacritics.
    diacritics: Option<bool>,
}

impl CalendarTab {
    pub fn ui(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        let diacritics = *self.diacritics.get_or_insert_with(|| {
            ui.ctx()
                .fonts_mut(|f| f.has_glyphs(&egui::FontId::proportional(14.0), DIACRITIC_SAMPLE))
        });
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Hijri calendar");
            calendar_selector(ui, &mut tc.settings.calendar);
            ui.add(egui::Label::new(egui::RichText::new(SUNSET_NOTE).small().weak()).wrap());

            let cal = active_calendar(&tc.settings.calendar);
            let selected = selected_day(&tc.settings.time);
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label("Selected date:");
                let civil = CivilDate::from_jdn(selected, CIVIL);
                let weekday = Weekday::from_jdn(selected).name_english();
                match cal.to_hijri(selected) {
                    Some(h) => ui.strong(format_hijri(h, diacritics)),
                    None => ui.weak("outside this calendar's range"),
                };
                ui.label(format!("— {weekday} {} (local)", format_civil(civil)));
            });

            ui.add_space(6.0);
            egui::CollapsingHeader::new("Month view")
                .default_open(true)
                .show(ui, |ui| self.month_view(ui, tc, diacritics));
            egui::CollapsingHeader::new("Date converter")
                .default_open(true)
                .show(ui, |ui| self.converter(ui, tc, diacritics));
            egui::CollapsingHeader::new("Month starts under every calendar")
                .default_open(true)
                .show(ui, |ui| self.comparison(ui, tc, diacritics));
        });
    }

    fn month_view(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>, diacritics: bool) {
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

        let cal = active_calendar(cs);
        let primary = cs.primary;
        let week_start = cs.week_start.weekday();
        let selected = selected_day(&tc.settings.time);
        let today = today(&tc.settings.time);

        // Re-centre on the selected date when it, the layout or the
        // calendar changes; otherwise keep the month the user navigated to.
        let key = (selected, primary, cs.calendar_id.clone());
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
            ui.weak("This month is outside the selected calendar's range.");
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

    fn converter(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>, diacritics: bool) {
        let cal = active_calendar(&tc.settings.calendar);
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
                                None => ui.weak("outside this calendar's range"),
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

    fn comparison(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>, diacritics: bool) {
        let cal = active_calendar(&tc.settings.calendar);
        let selected = selected_day(&tc.settings.time);
        let following = cal.to_hijri(selected).map_or(1448, |h| h.year);
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

        let infos = calendars();
        let cals: Vec<&dyn HijriCalendar> = infos.iter().map(|i| i.calendar as _).collect();
        let rows = compare_month_starts(year, &cals);
        let current = &tc.settings.calendar.calendar_id;
        let warn = ui.visuals().warn_fg_color;

        egui::ScrollArea::horizontal()
            .id_salt("comparison_scroll")
            .show(ui, |ui| {
                egui::Grid::new("comparison")
                    .striped(true)
                    .spacing([10.0, 4.0])
                    .show(ui, |ui| {
                        ui.strong("Month");
                        for info in infos {
                            let label = egui::RichText::new(short_label(info.id));
                            let label = if info.id == current {
                                label.strong().underline()
                            } else {
                                label.strong()
                            };
                            ui.label(label)
                                .on_hover_text(format!("{}\n{}", info.name, info.description));
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
fn calendar_selector(ui: &mut egui::Ui, cs: &mut CalendarSettings) {
    let info = selected_info(&cs.calendar_id);
    if info.id != cs.calendar_id {
        cs.calendar_id = info.id.to_owned();
    }
    ui.horizontal(|ui| {
        ui.label("Calendar:");
        egui::ComboBox::from_id_salt("hijri_calendar")
            .selected_text(info.name)
            .width(280.0)
            .show_ui(ui, |ui| {
                for c in calendars() {
                    ui.selectable_value(&mut cs.calendar_id, c.id.to_owned(), c.name)
                        .on_hover_text(c.description);
                }
                ui.separator();
                ui.add_enabled(
                    false,
                    egui::Button::selectable(false, "Visibility-based (coming soon)"),
                )
                .on_disabled_hover_text(
                    "Month starts predicted from crescent visibility at the observer's \
                         location under the selected criterion.",
                );
            });
    });
    let info = selected_info(&cs.calendar_id);
    ui.add(egui::Label::new(info.description).wrap());
    if let Some(credit) = info.attribution {
        ui.add(egui::Label::new(egui::RichText::new(credit).small().weak()).wrap());
    }
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
        };
        let back: CalendarSettings =
            serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
        let partial: CalendarSettings = serde_json::from_str(r#"{"primary":"Gregorian"}"#).unwrap();
        assert_eq!(partial.calendar_id, DEFAULT_CALENDAR_ID);
        assert_eq!(partial.primary, PrimaryCalendar::Gregorian);
    }

    #[test]
    fn set_day_keeps_local_time_of_day() {
        // 2024-03-10 22:30 UT = 2024-03-11 01:30 at UTC+3.
        let mut t = TimeSettings {
            jd_ut: datetime::calendar_to_jd(2024, 3, 10.0 + 22.5 / 24.0),
            tz_offset_hours: 3.0,
            edit_local: true,
            start_at_now: false,
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
