//! **Map** tab: the world crescent-visibility map.
//!
//! Flow:
//!
//! 1. [`MapTab::poll`] derives a [`MapSpec`] (local date, evening/morning,
//!    criterion, resolution, ephemeris) from the settings. When it changes
//!    (debounced by [`DEBOUNCE`]) a compute job replaces any running one:
//!    `mooncalc_maps::engine` over every core ([`compute::compute_map`]).
//! 2. A finished map, or a change of display options only (projection,
//!    globe centre, layers, view size), starts a cheap render job
//!    ([`draw::render_view`]); no recomputation.
//! 3. The image goes to the [`MapView`] widget, with the projection it was
//!    rendered with, so hover and click map back to the right cell.
//!
//! The side panel shows the legend from the criterion's `zones()` with
//! cell counts, the map facts, the hovered cell, and the earliest-sighting
//! scan for the lunation ([`compute::compute_earliest`]).

pub mod compute;
pub mod draw;

use std::sync::Arc;
use std::time::{Duration, Instant as Clock};

use mooncalc_core::events::CalendarDate;
use mooncalc_maps::projection::{MapProjection, Projection};

use crate::backend::{self, ActiveEphemeris};
use crate::criteria::{self, CriterionOption};
use crate::export::{ExportDialog, ExportFormat};
use crate::format::{fmt_instant, fmt_lat_lon_short};
use crate::jobs::JobSlot;
use crate::settings::{Crescent, Settings};
use crate::widgets::map_view::{MapImage, MapView};

pub use compute::{ComputedMap, EarliestResult, EarliestSpec, MapSpec, Resolution};
pub use draw::{ExportOptions, MapSettings, ProjectionKind};

use super::TabContext;

/// Quiet time after the last input change before a recomputation starts.
pub const DEBOUNCE: Duration = Duration::from_millis(250);

/// Largest rendered view, in pixels (zoomed views render sharper up to it).
const MAX_VIEW_WIDTH: f32 = 4096.0;

/// Width of the quick render used while the globe is being dragged.
const PREVIEW_WIDTH: u32 = 720;

/// The local calendar date of the selected time (UT + time-zone offset):
/// every map cell is evaluated on its own evening of this date.
pub fn map_date(settings: &Settings) -> CalendarDate {
    CalendarDate::from_jd(settings.time.jd_local())
}

fn app_crescent(c: Crescent) -> mooncalc_app::dto::Crescent {
    match c {
        Crescent::Evening => mooncalc_app::dto::Crescent::Evening,
        Crescent::Morning => mooncalc_app::dto::Crescent::Morning,
    }
}

/// When the evening (morning) a map of `date` shows under the observer is
/// not their civil one of `date` (east of 180° in UTC+13/+14): the note,
/// see `mooncalc_app::dateline::observer_note`.
pub fn observer_note(
    settings: &Settings,
    date: CalendarDate,
    crescent: Crescent,
) -> Option<String> {
    mooncalc_app::dateline::observer_note(
        date,
        settings.location.lon_deg,
        settings.time.tz_offset_hours,
        app_crescent(crescent),
    )
}

#[derive(Clone, Debug, PartialEq)]
struct RenderKey {
    map_id: u64,
    display: draw::DisplayKey,
    width: u32,
    preview: bool,
}

struct Rendered {
    image: MapImage,
    projection: MapProjection,
    map: Arc<ComputedMap>,
}

pub struct MapTab {
    view: MapView,
    criteria: Vec<CriterionOption>,
    compute: JobSlot<ComputedMap>,
    map: Option<Arc<ComputedMap>>,
    map_id: u64,
    submitted: Option<MapSpec>,
    pending: Option<(MapSpec, Clock)>,
    render: JobSlot<Rendered>,
    render_submitted: Option<RenderKey>,
    /// The map and projection of the image on screen.
    shown: Option<(MapProjection, Arc<ComputedMap>)>,
    error: Option<String>,
    /// Render width wanted by the view (physical pixels).
    view_width: u32,
    dragging: bool,
    hover: Option<(f64, f64)>,
    earliest: JobSlot<EarliestResult>,
    earliest_result: Option<EarliestResult>,
    earliest_submitted: Option<EarliestSpec>,
    earliest_error: Option<String>,
    export: ExportDialog,
}

impl Default for MapTab {
    fn default() -> Self {
        Self::new()
    }
}

impl MapTab {
    pub fn new() -> Self {
        Self {
            view: MapView::new("world_visibility_map"),
            criteria: criteria::criterion_options(),
            compute: JobSlot::new(),
            map: None,
            map_id: 0,
            submitted: None,
            pending: None,
            render: JobSlot::new(),
            render_submitted: None,
            shown: None,
            error: None,
            view_width: 1600,
            dragging: false,
            hover: None,
            earliest: JobSlot::new(),
            earliest_result: None,
            earliest_submitted: None,
            earliest_error: None,
            export: ExportDialog::default(),
        }
    }

    /// The most recently computed map.
    pub fn map(&self) -> Option<&Arc<ComputedMap>> {
        self.map.as_ref()
    }

    /// Open the export window (File menu).
    pub fn open_export(&mut self, format: ExportFormat) {
        self.export.open(format);
    }

    fn spec(settings: &Settings, active: &ActiveEphemeris) -> MapSpec {
        MapSpec {
            date: map_date(settings),
            crescent: settings.crescent,
            criterion_id: settings.criterion_id.clone(),
            resolution: settings.map.resolution,
            ephemeris: active.eph.name().to_owned(),
        }
    }

    /// Start recomputations when the inputs change; collect finished maps
    /// and renders.
    pub fn poll(&mut self, ctx: &egui::Context, tc: &mut TabContext<'_>) {
        let active = backend::resolve(tc.settings.backend, &tc.settings.de440_path);
        let spec = Self::spec(tc.settings, &active);

        // Debounced compute.
        if self.submitted.as_ref() != Some(&spec)
            && self.pending.as_ref().is_none_or(|(p, _)| *p != spec)
        {
            self.pending = Some((spec, Clock::now()));
        } else if self.submitted.as_ref() == Some(&spec) {
            self.pending = None;
        }
        if let Some((pending, since)) = &self.pending {
            let wait = if self.map.is_none() && !self.compute.is_running() {
                Duration::ZERO
            } else {
                DEBOUNCE
            };
            let elapsed = since.elapsed();
            if elapsed >= wait {
                let spec = pending.clone();
                self.pending = None;
                self.submit_compute(ctx, spec, &active);
            } else {
                ctx.request_repaint_after(wait - elapsed);
            }
        }

        match self.compute.poll() {
            Some(Ok(map)) => {
                self.map = Some(Arc::new(map));
                self.map_id += 1;
                self.error = None;
            }
            Some(Err(e)) => {
                tc.status.error(format!("Map: {e}"));
                self.error = Some(e);
            }
            None => {}
        }
        if let Some(progress) = self.compute.progress() {
            tc.status.report_job(progress);
        }

        self.poll_render(ctx, &tc.settings.map);
        self.poll_earliest(ctx, tc, &active);
        self.export.poll(tc.status);
    }

    fn submit_compute(&mut self, ctx: &egui::Context, spec: MapSpec, active: &ActiveEphemeris) {
        self.submitted = Some(spec.clone());
        if let Some(problem) = active.coverage_problem(spec.date.jd0() + 1.0, 3.0) {
            self.compute.cancel();
            self.error = Some(problem);
            return;
        }
        let eph = active.eph.clone();
        let repaint = ctx.clone();
        let label = format!(
            "Computing {} map ({})",
            spec.criterion_id,
            spec.resolution.label()
        );
        self.compute.submit(
            label,
            move || repaint.request_repaint(),
            move |job| compute::compute_map(spec, eph.as_ref(), job),
        );
    }

    fn poll_render(&mut self, ctx: &egui::Context, settings: &MapSettings) {
        match self.render.poll() {
            Some(Ok(r)) => {
                self.view.set_image(r.image);
                self.shown = Some((r.projection, r.map));
            }
            Some(Err(e)) => self.error = Some(format!("Rendering failed: {e}")),
            None => {}
        }
        let Some(map) = &self.map else { return };
        let preview = self.dragging && settings.projection == ProjectionKind::Globe;
        let key = RenderKey {
            map_id: self.map_id,
            display: settings.display_key(),
            width: if preview {
                PREVIEW_WIDTH.min(self.view_width)
            } else {
                self.view_width
            },
            preview,
        };
        // One render at a time; the latest settings win once it finishes.
        if self.render_submitted.as_ref() == Some(&key) || self.render.is_running() {
            return;
        }
        self.render_submitted = Some(key.clone());
        let (map, settings) = (map.clone(), settings.clone());
        let repaint = ctx.clone();
        self.render.submit(
            "Rendering map",
            move || repaint.request_repaint(),
            move |_job| {
                let img = draw::render_view(&map, &settings, key.width, key.preview);
                let image = MapImage::new(img.width as usize, img.height as usize, img.pixels)
                    .ok_or_else(|| crate::jobs::JobError::Failed("bad image size".into()))?;
                Ok(Rendered {
                    image,
                    projection: settings.projection(),
                    map,
                })
            },
        );
    }

    fn earliest_spec(settings: &Settings, active: &ActiveEphemeris) -> EarliestSpec {
        EarliestSpec {
            date: map_date(settings),
            criterion_id: settings.criterion_id.clone(),
            step_deg: settings.map.resolution.step_deg().max(1.0),
            ephemeris: active.eph.name().to_owned(),
        }
    }

    fn poll_earliest(
        &mut self,
        ctx: &egui::Context,
        tc: &mut TabContext<'_>,
        active: &ActiveEphemeris,
    ) {
        match self.earliest.poll() {
            Some(Ok(r)) => {
                self.earliest_result = Some(r);
                self.earliest_error = None;
            }
            Some(Err(e)) => self.earliest_error = Some(e),
            None => {}
        }
        if let Some(p) = self.earliest.progress() {
            tc.status.report_job(p);
        }
        // Runs after the map, so it never delays it.
        let spec = Self::earliest_spec(tc.settings, active);
        if self.earliest_submitted.as_ref() == Some(&spec)
            || self.pending.is_some()
            || self.compute.is_running()
        {
            return;
        }
        self.earliest_submitted = Some(spec.clone());
        if let Some(problem) = active.coverage_problem(spec.date.jd0() + 15.0, 20.0) {
            self.earliest.cancel();
            self.earliest_error = Some(problem);
            return;
        }
        let eph = active.eph.clone();
        let repaint = ctx.clone();
        self.earliest.submit(
            "Earliest sighting scan",
            move || repaint.request_repaint(),
            move |job| compute::compute_earliest(spec, eph.as_ref(), job),
        );
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        self.toolbar(ui, tc);
        ui.separator();

        egui::Panel::right("map_side_panel")
            .resizable(true)
            .default_size(330.0)
            .min_size(240.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.side_panel(ui, tc);
                });
            });

        let note = self
            .shown
            .as_ref()
            .and_then(|(_, m)| observer_note(tc.settings, m.spec.date, m.spec.crescent));
        egui::Panel::bottom("map_hover_strip").show(ui, |ui| {
            self.hover_strip(ui, note.as_deref());
        });

        if let Some(e) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, format!("Map: {e}"));
        }
        self.map_view(ui, tc);

        let ctx = ui.ctx().clone();
        self.export
            .ui(&ctx, self.map.as_ref(), &mut tc.settings.map);
    }

    fn map_view(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        let projection = self
            .shown
            .as_ref()
            .map_or_else(|| tc.settings.map.projection(), |(p, _)| *p);
        self.view.rotate_on_drag = tc.settings.map.projection == ProjectionKind::Globe;
        let inverse = |x: f32, y: f32| projection.inverse(f64::from(x), f64::from(y));
        let forward = |lat: f64, lon: f64| {
            projection
                .forward(lat, lon)
                .filter(|&(x, y)| (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y))
                .map(|(x, y)| (x as f32, y as f32))
        };
        let marker = Some((tc.settings.location.lat_deg, tc.settings.location.lon_deg));
        let r = self.view.show(ui, &inverse, Some(&forward), marker);

        let zoom = self.view.transform().zoom.min(3.0);
        let want = (r.fitted_px.x * zoom).clamp(256.0, MAX_VIEW_WIDTH);
        // Round up to 128 px so small resizes don't re-render.
        self.view_width = ((want / 128.0).ceil() * 128.0) as u32;
        self.dragging = r.dragging;

        if let Some(d) = r.drag {
            let m = &mut tc.settings.map;
            // The disc spans the image width: a full-width drag is 2 radians.
            let k = 2.0_f64.to_degrees();
            m.globe_lon_deg = crate::location::normalize_lon(m.globe_lon_deg - f64::from(d.x) * k);
            m.globe_lat_deg = (m.globe_lat_deg + f64::from(d.y) * k).clamp(-90.0, 90.0);
        }
        tc.status.set_hover(r.hover);
        self.hover = r.hover;
        if let Some((lat, lon)) = r.clicked {
            let loc = &mut tc.settings.location;
            loc.lat_deg = (lat * 1e4).round() / 1e4;
            loc.lon_deg = (lon * 1e4).round() / 1e4;
            loc.name = "Map location".into();
            tc.status.notify(format!(
                "Observer set to {}",
                crate::location::format_lat_lon(loc.lat_deg, loc.lon_deg)
            ));
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        let s = &mut *tc.settings;
        ui.horizontal_wrapped(|ui| {
            ui.strong("Date");
            if ui.small_button("<").on_hover_text("Previous day").clicked() {
                s.time.jd_ut -= 1.0;
            }
            ui.monospace(draw::date_text(map_date(s)))
                .on_hover_text(format!(
                    "Each place is evaluated on its own local evening/morning of this date \
                 (the selected time in {}).",
                    crate::datetime::format_utc_offset(s.time.tz_offset_hours)
                ));
            if ui.small_button(">").on_hover_text("Next day").clicked() {
                s.time.jd_ut += 1.0;
            }
            ui.separator();
            ui.selectable_value(&mut s.crescent, Crescent::Evening, "Evening")
                .on_hover_text(Crescent::Evening.label());
            ui.selectable_value(&mut s.crescent, Crescent::Morning, "Morning")
                .on_hover_text(Crescent::Morning.label());
            ui.separator();
            let current = criteria::find(&self.criteria, &s.criterion_id)
                .map_or_else(|| s.criterion_id.clone(), |o| o.name.clone());
            egui::ComboBox::from_id_salt("map_criterion")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for o in &self.criteria {
                        ui.selectable_value(&mut s.criterion_id, o.id.clone(), &o.name)
                            .on_hover_text(&o.reference);
                    }
                });
            ui.separator();
            ui.label("Grid");
            egui::ComboBox::from_id_salt("map_resolution")
                .selected_text(s.map.resolution.label())
                .width(60.0)
                .show_ui(ui, |ui| {
                    for r in Resolution::ALL {
                        ui.selectable_value(&mut s.map.resolution, r, r.label());
                    }
                });
        });
        ui.horizontal_wrapped(|ui| {
            let m = &mut s.map;
            egui::ComboBox::from_id_salt("map_projection")
                .selected_text(m.projection.label())
                .show_ui(ui, |ui| {
                    for p in ProjectionKind::ALL {
                        ui.selectable_value(&mut m.projection, p, p.label());
                    }
                });
            match m.projection {
                ProjectionKind::Globe => {
                    ui.label("Centre");
                    ui.add(
                        egui::DragValue::new(&mut m.globe_lat_deg)
                            .range(-90.0..=90.0)
                            .speed(0.5)
                            .suffix("° lat"),
                    );
                    ui.add(
                        egui::DragValue::new(&mut m.globe_lon_deg)
                            .range(-180.0..=180.0)
                            .speed(0.5)
                            .suffix("° lon"),
                    );
                    if ui
                        .small_button("Observer")
                        .on_hover_text("Centre the globe on the observer")
                        .clicked()
                    {
                        m.globe_lat_deg = s.location.lat_deg;
                        m.globe_lon_deg = s.location.lon_deg;
                    }
                }
                _ => {
                    ui.label("Centre");
                    ui.add(
                        egui::DragValue::new(&mut m.center_lon_deg)
                            .range(-180.0..=180.0)
                            .speed(1.0)
                            .suffix("° lon"),
                    );
                }
            }
            ui.separator();
            ui.checkbox(&mut m.contours, "Contours").on_hover_text(
                "Zone edges from the criterion's continuous value (Yallop q, Odeh V, ...) \
                 where it has one; otherwise smoothed between cells",
            );
            ui.checkbox(&mut m.borders, "Borders");
            ui.checkbox(&mut m.graticule, "Graticule");
            ui.checkbox(&mut m.cities, "Cities");
            ui.checkbox(&mut m.legend_in_image, "Legend on map");
            ui.add(
                egui::Slider::new(&mut m.zone_opacity, 0.2..=1.0)
                    .text("Opacity")
                    .fixed_decimals(2),
            );
            ui.separator();
            if ui.button("Export…").clicked() {
                self.export.open(ExportFormat::Png);
            }
        });
    }

    fn side_panel(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        let tz = tc.settings.time.tz_offset_hours;
        let Some(map) = self
            .shown
            .as_ref()
            .map(|(_, m)| m.clone())
            .or(self.map.clone())
        else {
            if self.compute.is_running() || self.pending.is_some() {
                ui.spinner();
                ui.label("Computing the first map…");
            } else if let Some(e) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, e);
            } else {
                ui.weak("No map yet.");
            }
            return;
        };
        ui.heading(&map.criterion_name);
        if self.compute.is_running() || self.pending.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.weak("Updating…");
            });
        }
        egui::Grid::new("map_info")
            .num_columns(2)
            .spacing([8.0, 3.0])
            .show(ui, |ui| {
                ui.label("Date");
                ui.label(format!(
                    "{} ({})",
                    draw::date_text(map.spec.date),
                    match map.spec.crescent {
                        Crescent::Evening => "evening",
                        Crescent::Morning => "morning",
                    }
                ));
                ui.end_row();
                ui.label("New Moon");
                ui.label(fmt_instant(map.conjunction.jd_ut, tz));
                ui.end_row();
                ui.label("Grid");
                ui.label(format!(
                    "{} ({} × {} = {} cells)",
                    map.spec.resolution.label(),
                    map.grid.cols,
                    map.grid.rows,
                    map.cells()
                ));
                ui.end_row();
                ui.label("Computed");
                ui.label(format!("{:.0} ms", map.elapsed.as_secs_f64() * 1e3));
                ui.end_row();
                ui.label("Ephemeris");
                ui.label(&map.spec.ephemeris);
                ui.end_row();
            });
        ui.add(
            egui::Label::new(
                egui::RichText::new(mooncalc_app::dateline::map_convention(
                    map.spec.date,
                    app_crescent(map.spec.crescent),
                ))
                .weak(),
            )
            .wrap(),
        );
        if let Some(note) = observer_note(tc.settings, map.spec.date, map.spec.crescent) {
            ui.add(
                egui::Label::new(egui::RichText::new(note).color(ui.visuals().warn_fg_color))
                    .wrap(),
            );
        }

        ui.add_space(6.0);
        ui.strong("Legend");
        let total = map.cells().max(1) as f64;
        egui::Grid::new("map_legend")
            .num_columns(3)
            .spacing([6.0, 3.0])
            .striped(true)
            .show(ui, |ui| {
                for (i, z) in map.zones.iter().enumerate() {
                    swatch(ui, z.rgb);
                    ui.add(egui::Label::new(format!("{}: {}", z.code, z.description)).wrap());
                    let n = map.zone_counts[i];
                    ui.monospace(format!("{n:>6} {:>5.1}%", 100.0 * n as f64 / total));
                    ui.end_row();
                }
                for (i, &n) in map.status_counts.iter().enumerate() {
                    if n == 0 {
                        continue;
                    }
                    let status = STATUSES[i];
                    ui.label("");
                    ui.weak(compute::status_text(status, map.spec.crescent));
                    ui.monospace(format!("{n:>6} {:>5.1}%", 100.0 * n as f64 / total));
                    ui.end_row();
                }
            });
        ui.weak("Counts are grid cells (not area-weighted).");

        ui.add_space(6.0);
        self.earliest_panel(ui, tc);
    }

    fn hover_strip(&self, ui: &mut egui::Ui, observer_note: Option<&str>) {
        let text = match (self.hover, self.shown.as_ref()) {
            (Some((lat, lon)), Some((_, map))) => {
                let mut t = format!("{}  ", fmt_lat_lon_short(lat, lon));
                match map.cell_at(lat, lon) {
                    Some(c) => match c.zone.and_then(|z| map.zone_info(z)) {
                        Some(z) => {
                            t.push_str(&format!("Zone {}: {}", z.code, z.description));
                            if let Some(v) = c.value {
                                t.push_str(&format!("   value {v:+.3}"));
                            }
                        }
                        None => t.push_str(compute::status_text(c.status, map.spec.crescent)),
                    },
                    None => t.push('—'),
                }
                t
            }
            _ => "Hover over the map for the zone and value of a place; click to set the \
                  observer."
                .to_owned(),
        };
        ui.horizontal(|ui| {
            if let (Some((lat, lon)), Some((_, map))) = (self.hover, self.shown.as_ref())
                && let Some(z) = map
                    .cell_at(lat, lon)
                    .and_then(|c| c.zone)
                    .and_then(|z| map.zone_info(z))
            {
                swatch(ui, z.rgb);
            }
            ui.label(text);
        });
        if let Some(note) = observer_note {
            ui.add(
                egui::Label::new(egui::RichText::new(note).color(ui.visuals().warn_fg_color))
                    .wrap(),
            );
        }
    }

    fn earliest_panel(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        let tz = tc.settings.time.tz_offset_hours;
        egui::CollapsingHeader::new("Earliest sighting (evening crescent)")
            .default_open(true)
            .show(ui, |ui| {
                if let Some(e) = &self.earliest_error {
                    ui.colored_label(ui.visuals().error_fg_color, e);
                }
                let running = self.earliest.is_running();
                let Some(r) = &self.earliest_result else {
                    if running {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Scanning…");
                        });
                    }
                    return;
                };
                ui.label(format!("New Moon {}", fmt_instant(r.conjunction.jd_ut, tz)));
                ui.weak(format!(
                    "First evening each zone (or better) appears anywhere, scanning {} to {} \
                     on a {}° grid.",
                    draw::date_text(r.dates[0]),
                    draw::date_text(*r.dates.last().unwrap_or(&r.dates[0])),
                    r.spec.step_deg
                ));
                if running {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.weak("Updating…");
                    });
                }
                let mut jump: Option<(CalendarDate, Option<(f64, f64)>)> = None;
                egui::Grid::new("earliest_grid")
                    .num_columns(4)
                    .spacing([6.0, 3.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for (i, first) in r.first.iter().enumerate() {
                            let z = &r.zones[i];
                            ui.horizontal(|ui| {
                                swatch(ui, z.rgb);
                                ui.label(z.code).on_hover_text(z.description);
                            });
                            match first {
                                Some(f) => {
                                    ui.monospace(draw::date_text(f.date));
                                    ui.label(fmt_lat_lon_short(f.lat_deg, f.lon_deg))
                                        .on_hover_text(format!(
                                            "Best cell{}; {} cells reach this zone",
                                            f.value
                                                .map(|v| format!(" (value {v:+.3})"))
                                                .unwrap_or_default(),
                                            f.cells
                                        ));
                                    ui.horizontal(|ui| {
                                        if ui
                                            .small_button("Go")
                                            .on_hover_text("Show this date's evening map")
                                            .clicked()
                                        {
                                            jump = Some((f.date, None));
                                        }
                                        if ui
                                            .small_button("📍")
                                            .on_hover_text(
                                                "Show this date and move the observer here",
                                            )
                                            .clicked()
                                        {
                                            jump = Some((f.date, Some((f.lat_deg, f.lon_deg))));
                                        }
                                    });
                                }
                                None => {
                                    ui.weak("not reached");
                                    ui.label("");
                                    ui.label("");
                                }
                            }
                            ui.end_row();
                        }
                    });
                if let Some((date, place)) = jump {
                    let s = &mut *tc.settings;
                    // 18:00 local on that date: the map shows that date's
                    // evenings, and the report that evening.
                    s.time.jd_ut = date.jd0() + 0.75 - s.time.tz_offset_hours / 24.0;
                    s.crescent = Crescent::Evening;
                    if let Some((lat, lon)) = place {
                        s.location.lat_deg = lat;
                        s.location.lon_deg = lon;
                        s.location.name = "Earliest sighting".into();
                    }
                }
            });
    }
}

use mooncalc_visibility::circumstances::Status;

/// Every [`Status`], indexed by its discriminant.
const STATUSES: [Status; compute::STATUS_COUNT] = [
    Status::Crescent,
    Status::BeforeConjunction,
    Status::MoonSetsFirst,
    Status::MoonAlwaysUp,
    Status::MoonAlwaysDown,
    Status::MidnightSun,
    Status::PolarNight,
    Status::SunAltitudeNotReached,
    Status::OutsideEphemeris,
];

/// A small colour square.
pub fn swatch(ui: &mut egui::Ui, rgb: [u8; 3]) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 2.0, egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
    ui.painter().rect_stroke(
        rect,
        2.0,
        egui::Stroke::new(1.0, ui.visuals().weak_text_color()),
        egui::StrokeKind::Inside,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_are_indexed_by_discriminant() {
        for (i, s) in STATUSES.iter().enumerate() {
            assert_eq!(*s as usize, i);
        }
    }

    /// One headless egui frame of the Map tab.
    fn frame(
        ctx: &egui::Context,
        tab: &mut MapTab,
        settings: &mut Settings,
        status: &mut crate::status::StatusInfo,
        events: Vec<egui::Event>,
    ) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 800.0),
            )),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            status.begin_frame();
            let mut tc = TabContext { settings, status };
            tab.poll(ui.ctx(), &mut tc);
            tab.ui(ui, &mut tc);
        });
        // No renderer: discard the texture uploads.
        out.textures_delta.clear();
    }

    #[test]
    fn hover_reports_the_cell_and_click_sets_the_observer() {
        let ctx = egui::Context::default();
        let mut settings = Settings {
            backend: crate::backend::BackendChoice::Meeus,
            ..Settings::default()
        };
        settings.time.jd_ut = CalendarDate::new(2024, 3, 10).jd0() + 0.75;
        settings.time.tz_offset_hours = 0.0;
        settings.map.resolution = Resolution::Deg2;
        let mut status = crate::status::StatusInfo::default();
        let mut tab = MapTab::new();
        let start = Clock::now();
        while tab.shown.is_none() {
            assert!(start.elapsed() < Duration::from_secs(60), "map never shown");
            frame(&ctx, &mut tab, &mut settings, &mut status, vec![]);
            std::thread::sleep(Duration::from_millis(10));
        }
        frame(&ctx, &mut tab, &mut settings, &mut status, vec![]);
        let (projection, map) = tab.shown.clone().unwrap();
        assert_eq!(map.spec.date, CalendarDate::new(2024, 3, 10));

        // Hover over the central Pacific: zone A or B.
        let (x, y) = projection.forward(15.0, -165.0).unwrap();
        let pos = tab.view.screen_pos(x as f32, y as f32).unwrap();
        frame(
            &ctx,
            &mut tab,
            &mut settings,
            &mut status,
            vec![egui::Event::PointerMoved(pos)],
        );
        let (lat, lon) = tab.hover.expect("hover");
        assert!(
            (lat - 15.0).abs() < 0.5 && (lon + 165.0).abs() < 0.5,
            "{lat} {lon}"
        );
        let cell = map.cell_at(lat, lon).unwrap();
        assert!(cell.zone.is_some_and(|z| z <= 1), "{cell:?}");

        // Click there: the observer moves.
        for pressed in [true, false] {
            frame(
                &ctx,
                &mut tab,
                &mut settings,
                &mut status,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        assert_eq!(settings.location.name, "Map location");
        assert!((settings.location.lat_deg - 15.0).abs() < 0.5);
        assert!((settings.location.lon_deg + 165.0).abs() < 0.5);
    }

    #[test]
    fn map_date_uses_the_local_date() {
        let mut s = Settings::default();
        s.time.jd_ut = CalendarDate::new(2024, 3, 10).jd0() + 22.0 / 24.0;
        s.time.tz_offset_hours = 0.0;
        assert_eq!(map_date(&s), CalendarDate::new(2024, 3, 10));
        s.time.tz_offset_hours = 3.0;
        assert_eq!(map_date(&s), CalendarDate::new(2024, 3, 11));
    }

    #[test]
    fn observer_note_where_the_civil_date_differs() {
        let mut s = Settings::default();
        let d = CalendarDate::new(2024, 3, 10);
        // Apia, UTC+13: the map for 10 March shows its civil 11 March.
        s.location.lon_deg = -171.7667;
        s.time.tz_offset_hours = 13.0;
        let note = observer_note(&s, d, Crescent::Evening).expect("note");
        assert!(
            note.contains("2024-03-11") && note.contains("2024-03-09"),
            "{note}"
        );
        // London: none.
        s.location.lon_deg = -0.13;
        s.time.tz_offset_hours = 0.0;
        assert!(observer_note(&s, d, Crescent::Evening).is_none());
    }
}
