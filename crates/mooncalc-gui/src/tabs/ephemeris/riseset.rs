//! Rise/set table: a month of daily Sun and Moon rise, upper transit and
//! set (with azimuths and the transit altitude), plus civil, nautical and
//! astronomical twilight, for the app location in the app time zone.
//!
//! Events come from `mooncalc_core::events` (`day_events`, `twilight`):
//! upper limb on the horizon with 34′ refraction, Sun's centre at −6°,
//! −12°, −18° for twilight. Positions are interpolated from an
//! `ApparentTable` built once per month.

use mooncalc_atlas::Zone;
use mooncalc_core::events::{
    ApparentTable, Body, CalendarDate, Circumpolar, Horizon, Observer, PositionSource, Twilight,
    day_events, twilight,
};
use mooncalc_core::refraction::{Atmosphere, apparent_altitude};
use mooncalc_core::{GeoLocation, Instant};

use super::cached::CachedJob;
#[cfg(test)]
use super::format;
use super::{DAY_MARGIN, coverage};
use super::{EphKey, busy, ephemeris, error_line, set_app_time, time_cell};
use crate::datetime::{DateTimeFields, days_in_month};
use crate::jobs::JobError;
use crate::tabs::TabContext;
use crate::widgets::coverage::coverage_notice;

const MONTHS: [&str; 12] = [
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

/// An event with an angle (azimuth for rise/set, altitude for transit).
pub type AngledEvent = Option<(Instant, f64)>;

/// One body's events on one local day.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BodyDay {
    pub rise: AngledEvent,
    pub transit: AngledEvent,
    pub set: AngledEvent,
    pub circumpolar: Option<Circumpolar>,
}

/// One local day of the table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DayRow {
    pub date: CalendarDate,
    pub sun: BodyDay,
    pub moon: BodyDay,
    /// (morning begin, evening end) of each twilight.
    pub civil: (Option<Instant>, Option<Instant>),
    pub nautical: (Option<Instant>, Option<Instant>),
    pub astronomical: (Option<Instant>, Option<Instant>),
}

#[derive(Clone, Debug, PartialEq)]
struct Request {
    year: i32,
    month: u32,
    tz: Zone,
    location: (f64, f64, f64),
    eph: EphKey,
}

/// Compute the rows of one month.
pub fn month_rows<S: PositionSource + ?Sized>(
    src: &S,
    loc: &GeoLocation,
    year: i32,
    month: u32,
    tz: Zone,
    mut cancelled: impl FnMut() -> bool,
) -> Option<Vec<DayRow>> {
    let obs = Observer::new(loc);
    let atm = Atmosphere::STANDARD;
    let angle = |body: Body, t: Instant, azimuth: bool| -> f64 {
        let f = obs.frame_at(src, t.jd_ut);
        let v = f.topocentric(&src.position(body, t.jd_ut));
        if azimuth {
            f.azimuth(&v)
        } else {
            apparent_altitude(f.altitude(&v), &atm)
        }
    };
    let body_day = |body: Body, t0: Instant, t1: Instant| -> BodyDay {
        let d = day_events(src, body, loc, Horizon::STANDARD, t0, t1);
        BodyDay {
            rise: d.rise.map(|t| (t, angle(body, t, true))),
            transit: d.upper_transit.map(|t| (t, angle(body, t, false))),
            set: d.set.map(|t| (t, angle(body, t, true))),
            circumpolar: d.circumpolar,
        }
    };
    let mut rows = Vec::new();
    for day in 1..=days_in_month(year, month) {
        if cancelled() {
            return None;
        }
        let date = CalendarDate::new(year, month, day);
        let (t0, t1) = tz.day_bounds(date);
        rows.push(DayRow {
            date,
            sun: body_day(Body::Sun, t0, t1),
            moon: body_day(Body::Moon, t0, t1),
            civil: twilight(src, loc, Twilight::Civil, t0, t1),
            nautical: twilight(src, loc, Twilight::Nautical, t0, t1),
            astronomical: twilight(src, loc, Twilight::Astronomical, t0, t1),
        });
    }
    Some(rows)
}

/// Transient state of the rise/set view.
#[derive(Default)]
pub struct RiseSetView {
    job: CachedJob<Request, Vec<DayRow>>,
    /// Chosen (year, month); `None` follows the app date.
    month: Option<(i32, u32)>,
}

impl RiseSetView {
    fn year_month(&self, tc: &TabContext<'_>) -> (i32, u32) {
        self.month.unwrap_or_else(|| {
            let f = DateTimeFields::from_jd(tc.settings.time.jd_local());
            (f.year, f.month)
        })
    }

    /// The month's days, with the interpolation table's day either side.
    fn coverage(&self, tc: &TabContext<'_>) -> Option<String> {
        let (year, month) = self.year_month(tc);
        let tz = tc.settings.time.zone();
        let (start, _) = tz.day_bounds(CalendarDate::new(year, month, 1));
        let days = f64::from(days_in_month(year, month));
        coverage(
            tc.settings,
            start.jd_ut,
            start.jd_ut + days,
            DAY_MARGIN,
            DAY_MARGIN,
        )
    }

    pub fn poll(&mut self, ctx: &egui::Context, tc: &mut TabContext<'_>, visible: bool) {
        if visible && self.coverage(tc).is_none() {
            let (year, month) = self.year_month(tc);
            let l = &tc.settings.location;
            let req = Request {
                year,
                month,
                tz: tc.settings.time.zone(),
                location: (l.lat_deg, l.lon_deg, l.elevation_m),
                eph: EphKey::of(tc.settings),
            };
            let eph = ephemeris(tc.settings);
            self.job
                .request(ctx, req, "Computing rise/set table", move |req, job| {
                    let loc = GeoLocation::new(req.location.0, req.location.1, req.location.2);
                    let (start, _) = req.tz.day_bounds(CalendarDate::new(req.year, req.month, 1));
                    let days = days_in_month(req.year, req.month) as f64;
                    let table = ApparentTable::with_default_step(
                        eph.as_ref(),
                        start.add_days(-1.0),
                        start.add_days(days + 1.0),
                    );
                    let mut n = 0.0;
                    month_rows(&table, &loc, req.year, req.month, req.tz, || {
                        n += 1.0;
                        job.set_progress((n / days) as f32);
                        job.is_cancelled()
                    })
                    .ok_or(JobError::Cancelled)
                });
        }
        self.job.poll(tc.status);
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        let tz = tc.settings.time.zone();
        let (mut year, mut month) = self.year_month(tc);
        ui.horizontal_wrapped(|ui| {
            if ui
                .small_button("◀")
                .on_hover_text("Previous month")
                .clicked()
            {
                (year, month) = if month == 1 {
                    (year - 1, 12)
                } else {
                    (year, month - 1)
                };
            }
            egui::ComboBox::from_id_salt("riseset_month")
                .selected_text(MONTHS[month as usize - 1])
                .width(100.0)
                .show_ui(ui, |ui| {
                    for (i, name) in MONTHS.iter().enumerate() {
                        ui.selectable_value(&mut month, i as u32 + 1, *name);
                    }
                });
            ui.add(
                egui::DragValue::new(&mut year)
                    .range(-3000..=5000)
                    .speed(0.2),
            );
            if ui.small_button("▶").on_hover_text("Next month").clicked() {
                (year, month) = if month == 12 {
                    (year + 1, 1)
                } else {
                    (year, month + 1)
                };
            }
            if (year, month) != self.year_month(tc) {
                self.month = Some((year, month));
            }
            if self.month.is_some() && ui.small_button("App month").clicked() {
                self.month = None;
            }
            ui.separator();
            let l = &tc.settings.location;
            ui.label(format!(
                "{} — {}",
                l.name,
                crate::location::format_lat_lon(l.lat_deg, l.lon_deg)
            ));
        });
        ui.weak(
            "Rise/set: upper limb, 34′ refraction. Az = azimuth from north; transit altitude \
             includes refraction. Twilight: Sun's centre at −6° (civil), −12° (nautical), \
             −18° (astronomical).",
        );
        if let Some(problem) = self.coverage(tc) {
            coverage_notice(ui, &problem);
            return;
        }
        error_line(ui, &self.job.error);
        busy(ui, self.job.is_running());
        let Some((req, rows)) = self.job.value.as_ref() else {
            return;
        };
        let stale = req.year != year || req.month != month;

        let mut clicked = None;
        egui::ScrollArea::both().show(ui, |ui| {
            if stale {
                ui.disable();
            }
            egui::Grid::new("riseset_grid")
                .striped(true)
                .spacing([10.0, 3.0])
                .show(ui, |ui| {
                    for h in [
                        "Date", "Astro.", "Naut.", "Civil", "Sunrise", "Az", "Transit", "Alt",
                        "Sunset", "Az", "Civil", "Naut.", "Astro.", "Moonrise", "Az", "Transit",
                        "Alt", "Moonset", "Az",
                    ] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for r in rows {
                        ui.monospace(format!(
                            "{:04}-{:02}-{:02}",
                            r.date.year, r.date.month, r.date.day
                        ));
                        for t in [r.astronomical.0, r.nautical.0, r.civil.0] {
                            clicked = clicked.or(time_cell(ui, t, tz, false));
                        }
                        body_cells(ui, &r.sun, tz, &mut clicked);
                        for t in [r.civil.1, r.nautical.1, r.astronomical.1] {
                            clicked = clicked.or(time_cell(ui, t, tz, false));
                        }
                        body_cells(ui, &r.moon, tz, &mut clicked);
                        ui.end_row();
                    }
                });
        });
        if let Some(t) = clicked {
            set_app_time(tc, t);
        }
    }
}

fn body_cells(ui: &mut egui::Ui, d: &BodyDay, tz: Zone, clicked: &mut Option<Instant>) {
    for (ev, azimuth) in [(d.rise, true), (d.transit, false), (d.set, true)] {
        match ev {
            Some((t, a)) => {
                *clicked = clicked.or(time_cell(ui, Some(t), tz, false));
                let deg = a.to_degrees();
                ui.monospace(if azimuth {
                    format!("{deg:.0}°")
                } else {
                    format!("{deg:+.1}°")
                });
            }
            None => {
                let text = match (d.circumpolar, azimuth) {
                    (Some(Circumpolar::AlwaysUp), true) => "up",
                    (Some(Circumpolar::AlwaysDown), true) => "down",
                    _ => "—",
                };
                ui.weak(text);
                ui.label("");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mooncalc_core::MeeusEphemeris;
    use mooncalc_core::events::DirectSource;

    #[test]
    fn makkah_march_2024() {
        let eph = MeeusEphemeris::new();
        let src = DirectSource::new(&eph);
        let loc = GeoLocation::new(21.4225, 39.8262, 0.0);
        let rows = month_rows(&src, &loc, 2024, 3, Zone::fixed_hours(3.0), || false).unwrap();
        assert_eq!(rows.len(), 31);
        let r = &rows[10]; // 2024-03-11
        assert_eq!(r.date, CalendarDate::new(2024, 3, 11));
        // Sunrise ≈ 06:32, sunset ≈ 18:31 local (UTC+3) near the equinox;
        // azimuths near 93°/267°.
        let (rise, az) = r.sun.rise.unwrap();
        let (set, saz) = r.sun.set.unwrap();
        assert_eq!(format::time_hm(rise, Zone::fixed_hours(3.0))[..2], *"06");
        assert_eq!(format::time_hm(set, Zone::fixed_hours(3.0))[..2], *"18");
        assert!((az.to_degrees() - 93.0).abs() < 3.0, "{}", az.to_degrees());
        assert!((saz.to_degrees() - 267.0).abs() < 3.0);
        // Twilights are ordered around sunrise and sunset.
        assert!(r.astronomical.0.unwrap().jd_ut < r.nautical.0.unwrap().jd_ut);
        assert!(r.civil.0.unwrap().jd_ut < rise.jd_ut);
        assert!(r.civil.1.unwrap().jd_ut > set.jd_ut);
        // The day after new Moon (10 March) the crescent sets after the Sun.
        let (mset, _) = r.moon.set.unwrap();
        assert!(mset.jd_ut > set.jd_ut);
    }
}
