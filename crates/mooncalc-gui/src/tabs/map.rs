//! **Map** tab: the world crescent-visibility map.
//!
//! Flow: when [`Settings::map_request`] changes, [`MapTab::poll`] submits a
//! job (replacing any running one) that turns the request into an RGBA image;
//! the result is handed to the [`MapView`] widget. Hover reports lat/lon to
//! the status bar; a click sets the observer location.
//!
//! **Extension point (wave 2, grid engine + map view):** replace
//! [`compute_map_image`] with the real pipeline: build the ephemeris
//! ([`crate::backend::create_ephemeris`]), evaluate the criterion over a
//! `mooncalc_maps::grid` with [`crate::jobs::parallel_rows`], render it with
//! `mooncalc_maps::render`, and swap the projection callbacks for
//! `mooncalc_maps::projection`. Replace [`placeholder_map::ZONES`] in the
//! legend with the criterion's `zones()`.

use crate::jobs::{JobContext, JobError, JobSlot};
use crate::placeholder_map;
use crate::settings::{MapRequest, Settings};
use crate::widgets::map_view::{MapImage, MapView};

use super::TabContext;

/// Size of the computed map image, in pixels (equirectangular, 2:1).
pub const MAP_WIDTH: usize = 1440;
pub const MAP_HEIGHT: usize = 720;

/// **Extension point.** Compute the map image for a request, on a worker
/// thread. Check `job` for cancellation and report progress through it.
pub fn compute_map_image(req: &MapRequest, job: &JobContext) -> Result<MapImage, JobError> {
    placeholder_map::render(req, MAP_WIDTH, MAP_HEIGHT, job)
}

pub struct MapTab {
    view: MapView,
    job: JobSlot<MapImage>,
    /// The request the current (or running) image belongs to.
    requested: Option<MapRequest>,
    error: Option<String>,
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
            job: JobSlot::new(),
            requested: None,
            error: None,
        }
    }

    /// Start a recomputation when the inputs change; collect finished maps.
    pub fn poll(&mut self, ctx: &egui::Context, tc: &mut TabContext<'_>) {
        let request = tc.settings.map_request();
        if self.requested.as_ref() != Some(&request) {
            self.requested = Some(request.clone());
            let repaint_ctx = ctx.clone();
            self.job.submit(
                "Computing visibility map",
                move || repaint_ctx.request_repaint(),
                move |job| compute_map_image(&request, job),
            );
        }
        match self.job.poll() {
            Some(Ok(image)) => {
                self.view.set_image(image);
                self.error = None;
            }
            Some(Err(e)) => {
                tc.status.error(format!("Map: {e}"));
                self.error = Some(e);
            }
            None => {}
        }
        if let Some(progress) = self.job.progress() {
            tc.status.report_job(progress);
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        ui.horizontal_wrapped(|ui| {
            ui.heading("World visibility map");
            ui.separator();
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "Placeholder data: the visibility grid engine is not connected yet.",
            );
        });
        legend(ui, tc.settings);
        if let Some(e) = &self.error {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("Map computation failed: {e}"),
            );
        }

        let marker = Some((tc.settings.location.lat_deg, tc.settings.location.lon_deg));
        let response = self.view.show(
            ui,
            &placeholder_map::equirect_inverse,
            Some(&placeholder_map::equirect_forward),
            marker,
        );
        tc.status.set_hover(response.hover);
        if let Some((lat, lon)) = response.clicked {
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
}

fn legend(ui: &mut egui::Ui, settings: &Settings) {
    ui.horizontal_wrapped(|ui| {
        ui.label(format!("Criterion: {}", settings.criterion_id));
        ui.separator();
        for (code, description, rgb) in placeholder_map::ZONES {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(rect, 2.0, egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
            ui.label(code).on_hover_text(description);
        }
    });
}
