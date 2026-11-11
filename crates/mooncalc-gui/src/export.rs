//! Map export (File → Export map).
//!
//! **Extension point:** implement [`export_map`] with `mooncalc_maps::render`
//! (PNG via the `png` crate, SVG as text) once the maps-render package lands.
//! A save dialog needs a new dependency (e.g. `rfd`); until one is adopted,
//! write next to the working directory with a generated file name.

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
}

/// **Extension point.** Export the current map. Returns the written path.
pub fn export_map(format: ExportFormat) -> Result<std::path::PathBuf, String> {
    Err(format!(
        "{} export is not implemented yet (arrives with the map renderer)",
        format.extension().to_uppercase()
    ))
}
