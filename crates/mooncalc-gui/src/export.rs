//! Map export (File → Export map) and native file dialogs.
//!
//! [`ExportDialog`] is a small window owned by the Map tab: format, size,
//! legend and title. "Save…" opens the native save dialog (`rfd`), then a
//! background job renders the map with `mooncalc_maps::render` (PNG through
//! [`mooncalc_maps::render::encode_png`], SVG through
//! [`mooncalc_maps::render::render_svg`]) and writes the file.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::jobs::{JobError, JobSlot};
use crate::status::StatusInfo;
use crate::tabs::map::{ComputedMap, ExportOptions, MapSettings, draw};

/// Output format of a map export.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Png,
    Svg,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Png => "png",
            ExportFormat::Svg => "svg",
        }
    }

    pub fn menu_label(self) -> &'static str {
        match self {
            ExportFormat::Png => "Export map as PNG…",
            ExportFormat::Svg => "Export map as SVG…",
        }
    }

    fn filter_name(self) -> &'static str {
        match self {
            ExportFormat::Png => "PNG image",
            ExportFormat::Svg => "SVG image",
        }
    }
}

/// Width presets offered in the dialog, pixels.
pub const WIDTH_PRESETS: [u32; 5] = [1440, 2048, 2880, 4096, 8192];

/// Where exports go when the native dialog is unavailable: the user's
/// Pictures folder, else their home folder, else the working directory.
pub fn default_export_dir() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from);
    match home {
        Some(h) if h.join("Pictures").is_dir() => h.join("Pictures"),
        Some(h) if h.is_dir() => h,
        _ => PathBuf::from("."),
    }
}

/// `mooncalc_2024-03-10_evening_yallop_1deg.png`.
pub fn default_file_name(map: &ComputedMap, format: ExportFormat) -> String {
    let crescent = match map.spec.crescent {
        crate::settings::Crescent::Evening => "evening",
        crate::settings::Crescent::Morning => "morning",
    };
    let res = map.spec.resolution.label().replace('°', "deg");
    format!(
        "mooncalc_{}_{crescent}_{}_{res}.{}",
        draw::date_text(map.spec.date),
        map.spec.criterion_id,
        format.extension()
    )
}

/// Ask for a save path with the native dialog. `None` if cancelled.
pub fn ask_save_path(format: ExportFormat, file_name: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Export map")
        .set_directory(default_export_dir())
        .set_file_name(file_name)
        .add_filter(format.filter_name(), &[format.extension()])
        .save_file()
        .map(|p| with_extension(p, format))
}

/// Ask for an SPK (`.bsp`) file with the native dialog.
pub fn ask_bsp_path(current: &str) -> Option<PathBuf> {
    let mut d = rfd::FileDialog::new()
        .set_title("Choose a JPL DE440/DE440s file")
        .add_filter("SPK ephemeris", &["bsp"]);
    if let Some(dir) = Path::new(current.trim()).parent().filter(|p| p.is_dir()) {
        d = d.set_directory(dir);
    }
    d.pick_file()
}

fn with_extension(p: PathBuf, format: ExportFormat) -> PathBuf {
    if p.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(format.extension()))
    {
        p
    } else {
        let mut s = p.into_os_string();
        s.push(".");
        s.push(format.extension());
        PathBuf::from(s)
    }
}

/// Render and encode a map export.
pub fn export_bytes(
    map: &ComputedMap,
    settings: &MapSettings,
    opts: &ExportOptions,
    format: ExportFormat,
) -> Result<Vec<u8>, String> {
    match format {
        ExportFormat::Png => {
            let img = draw::export_raster(map, settings, opts);
            mooncalc_maps::render::encode_png(&img).map_err(|e| e.to_string())
        }
        ExportFormat::Svg => Ok(draw::export_svg(map, settings, opts).into_bytes()),
    }
}

/// Render a map export and write it to `path`.
pub fn write_export(
    map: &ComputedMap,
    settings: &MapSettings,
    opts: &ExportOptions,
    format: ExportFormat,
    path: &Path,
) -> Result<(), String> {
    let bytes = export_bytes(map, settings, opts, format)?;
    std::fs::write(path, bytes).map_err(|e| format!("could not write {}: {e}", path.display()))
}

/// The export window.
pub struct ExportDialog {
    open: bool,
    format: ExportFormat,
    job: JobSlot<PathBuf>,
}

impl Default for ExportDialog {
    fn default() -> Self {
        Self {
            open: false,
            format: ExportFormat::Png,
            job: JobSlot::new(),
        }
    }
}

impl ExportDialog {
    /// Show the dialog with `format` selected.
    pub fn open(&mut self, format: ExportFormat) {
        self.format = format;
        self.open = true;
    }

    /// Collect a finished export.
    pub fn poll(&mut self, status: &mut StatusInfo) {
        match self.job.poll() {
            Some(Ok(path)) => status.notify(format!("Map saved to {}", path.display())),
            Some(Err(e)) => status.error(format!("Export failed: {e}")),
            None => {}
        }
        if let Some(p) = self.job.progress() {
            status.report_job(p);
        }
    }

    /// Draw the window (if open).
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        map: Option<&Arc<ComputedMap>>,
        settings: &mut MapSettings,
    ) {
        let mut open = self.open;
        let mut save = false;
        egui::Window::new("Export map")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Format:");
                    ui.selectable_value(&mut self.format, ExportFormat::Png, "PNG");
                    ui.selectable_value(&mut self.format, ExportFormat::Svg, "SVG");
                });
                ui.horizontal(|ui| {
                    ui.label("Width:");
                    egui::ComboBox::from_id_salt("export_width")
                        .selected_text(format!("{} px", settings.export_width))
                        .show_ui(ui, |ui| {
                            for w in WIDTH_PRESETS {
                                ui.selectable_value(
                                    &mut settings.export_width,
                                    w,
                                    format!("{w} px"),
                                );
                            }
                        });
                    ui.add(
                        egui::DragValue::new(&mut settings.export_width)
                            .range(256..=16384)
                            .speed(16.0)
                            .suffix(" px"),
                    );
                });
                let aspect =
                    mooncalc_maps::projection::Projection::aspect_ratio(&settings.projection());
                ui.weak(format!(
                    "Map height {} px ({} projection)",
                    (f64::from(settings.export_width) / aspect).round(),
                    settings.projection.label()
                ));
                ui.checkbox(&mut settings.export_legend, "Legend");
                ui.checkbox(
                    &mut settings.export_title,
                    "Title (date, criterion, new Moon)",
                );
                if self.format == ExportFormat::Svg {
                    ui.weak("SVG zones follow the grid cells exactly (no contour smoothing).");
                }
                ui.separator();
                match map {
                    Some(map) => {
                        ui.label(draw::map_title(map));
                        ui.horizontal(|ui| {
                            let busy = self.job.is_running();
                            if ui.add_enabled(!busy, egui::Button::new("Save…")).clicked() {
                                save = true;
                            }
                            if busy {
                                ui.spinner();
                                ui.label("Exporting…");
                            }
                        });
                    }
                    None => {
                        ui.weak("No map has been computed yet.");
                    }
                }
            });
        self.open = open;
        if save && let Some(map) = map {
            let name = default_file_name(map, self.format);
            if let Some(path) = ask_save_path(self.format, &name) {
                self.start(ctx, map.clone(), settings.clone(), self.format, path);
            }
        }
    }

    fn start(
        &mut self,
        ctx: &egui::Context,
        map: Arc<ComputedMap>,
        settings: MapSettings,
        format: ExportFormat,
        path: PathBuf,
    ) {
        let opts = ExportOptions {
            width: settings.export_width.clamp(256, 16384),
            legend: settings.export_legend,
            title: settings.export_title,
        };
        let repaint = ctx.clone();
        self.job.submit(
            format!("Exporting {}", format.extension().to_uppercase()),
            move || repaint.request_repaint(),
            move |_job| {
                write_export(&map, &settings, &opts, format, &path).map_err(JobError::Failed)?;
                Ok(path)
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_paths_get_the_right_extension() {
        assert_eq!(
            with_extension(PathBuf::from("a/map"), ExportFormat::Png),
            PathBuf::from("a/map.png")
        );
        assert_eq!(
            with_extension(PathBuf::from("a/map.SVG"), ExportFormat::Svg),
            PathBuf::from("a/map.SVG")
        );
    }

    #[test]
    fn default_dir_exists() {
        assert!(default_export_dir().is_dir());
    }
}
