//! Turning a [`ComputedMap`] into pixels or SVG with `mooncalc_maps::render`:
//! the persisted display options ([`MapSettings`]), the projection, the
//! render options, city markers and the export composition (title band and
//! legend).

use mooncalc_maps::basemap::Scale;
use mooncalc_maps::projection::{Equirectangular, MapProjection, Orthographic, Robinson};
use mooncalc_maps::render::{
    Graticule, Legend, LegendEntry, LegendPosition, LineStyle, MapData, Marker, RenderOptions,
    RgbaImage, ZoneSmoothing, render_raster, render_svg,
};
use serde::{Deserialize, Serialize};

use super::compute::{ComputedMap, Resolution};
use crate::settings::Crescent;

/// Map projection choice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProjectionKind {
    #[default]
    Equirectangular,
    Robinson,
    Globe,
}

impl ProjectionKind {
    pub const ALL: [ProjectionKind; 3] = [
        ProjectionKind::Equirectangular,
        ProjectionKind::Robinson,
        ProjectionKind::Globe,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ProjectionKind::Equirectangular => "Equirectangular",
            ProjectionKind::Robinson => "Robinson",
            ProjectionKind::Globe => "Globe",
        }
    }
}

/// Persisted Map-tab options (`Settings::map`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MapSettings {
    pub resolution: Resolution,
    pub projection: ProjectionKind,
    /// Central meridian of the equirectangular and Robinson maps.
    pub center_lon_deg: f64,
    /// Centre of the globe view.
    pub globe_lat_deg: f64,
    pub globe_lon_deg: f64,
    /// Contour zones from the criterion's continuous value where possible
    /// (otherwise a smoothed vote between cells).
    pub contours: bool,
    pub borders: bool,
    pub graticule: bool,
    pub cities: bool,
    /// Draw the legend into the map image (the side panel always has one).
    pub legend_in_image: bool,
    pub zone_opacity: f32,
    /// Last export size (width in pixels) and options.
    pub export_width: u32,
    pub export_legend: bool,
    pub export_title: bool,
}

impl Default for MapSettings {
    fn default() -> Self {
        Self {
            resolution: Resolution::Deg1,
            projection: ProjectionKind::Equirectangular,
            center_lon_deg: 0.0,
            globe_lat_deg: 15.0,
            globe_lon_deg: -60.0,
            contours: true,
            borders: true,
            graticule: true,
            cities: false,
            legend_in_image: false,
            zone_opacity: 0.7,
            export_width: 2048,
            export_legend: true,
            export_title: true,
        }
    }
}

/// The display options that change the rendered image (everything except
/// export settings), for change detection.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayKey {
    pub projection: MapProjection,
    pub contours: bool,
    pub borders: bool,
    pub graticule: bool,
    pub cities: bool,
    pub legend_in_image: bool,
    pub zone_opacity: f32,
}

impl MapSettings {
    pub fn projection(&self) -> MapProjection {
        match self.projection {
            ProjectionKind::Equirectangular => Equirectangular::world(self.center_lon_deg).into(),
            ProjectionKind::Robinson => Robinson::new(self.center_lon_deg).into(),
            ProjectionKind::Globe => {
                Orthographic::new(self.globe_lat_deg, self.globe_lon_deg).into()
            }
        }
    }

    pub fn display_key(&self) -> DisplayKey {
        DisplayKey {
            projection: self.projection(),
            contours: self.contours,
            borders: self.borders,
            graticule: self.graticule,
            cities: self.cities,
            legend_in_image: self.legend_in_image,
            zone_opacity: self.zone_opacity,
        }
    }
}

/// Well-known cities for the optional markers: (name, lat, lon).
pub const CITIES: [(&str, f64, f64); 16] = [
    ("Makkah", 21.4225, 39.8262),
    ("Cairo", 30.0444, 31.2357),
    ("Istanbul", 41.0082, 28.9784),
    ("London", 51.5074, -0.1278),
    ("Lagos", 6.5244, 3.3792),
    ("Cape Town", -33.9249, 18.4241),
    ("Tehran", 35.6892, 51.389),
    ("Karachi", 24.8607, 67.0011),
    ("Delhi", 28.6139, 77.209),
    ("Dhaka", 23.8103, 90.4125),
    ("Kuala Lumpur", 3.139, 101.6869),
    ("Jakarta", -6.2088, 106.8456),
    ("Sydney", -33.8688, 151.2093),
    ("Los Angeles", 34.0522, -118.2437),
    ("New York", 40.7128, -74.006),
    ("Sao Paulo", -23.5505, -46.6333),
];

/// The 5x7 map font covers printable ASCII and `°` only.
pub fn ascii_label(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            ' '..='~' | '°' => out.push(c),
            '≥' => out.push_str(">="),
            '≤' => out.push_str("<="),
            '′' | '’' | '‘' => out.push('\''),
            '″' | '“' | '”' => out.push('"'),
            '–' | '—' | '−' => out.push('-'),
            '×' => out.push('x'),
            '·' => out.push('-'),
            'á' | 'à' | 'â' | 'ä' => out.push('a'),
            'é' | 'è' | 'ê' => out.push('e'),
            'ö' | 'ó' => out.push('o'),
            'ü' | 'ú' => out.push('u'),
            'ç' => out.push('c'),
            _ => out.push('?'),
        }
    }
    out
}

/// "2024-03-10".
pub fn date_text(d: mooncalc_core::events::CalendarDate) -> String {
    format!("{:04}-{:02}-{:02}", d.year, d.month, d.day)
}

/// One-line title: date, crescent, criterion, conjunction.
pub fn map_title(map: &ComputedMap) -> String {
    let c = map.conjunction.to_calendar();
    format!(
        "{} {} crescent - {} - new Moon {:04}-{:02}-{:02} {:02}:{:02} UT - {} grid",
        date_text(map.spec.date),
        match map.spec.crescent {
            Crescent::Evening => "evening",
            Crescent::Morning => "morning",
        },
        map.criterion_name,
        c.year,
        c.month,
        c.day,
        c.hour,
        c.minute,
        map.spec.resolution.label(),
    )
}

/// The zone legend of a map (ASCII labels).
pub fn legend(map: &ComputedMap, text_scale: u32, title: bool) -> Legend {
    Legend {
        title: title.then(|| ascii_label(&map.criterion_name)),
        entries: map
            .zones
            .iter()
            .map(|z| LegendEntry {
                color: z.rgb,
                label: ascii_label(&format!("{}: {}", z.code, z.description)),
            })
            .collect(),
        position: LegendPosition::BottomLeft,
        text_scale: text_scale.max(1),
        ..Legend::default()
    }
}

/// Render options for a map image `width` pixels wide (height from the
/// projection's aspect ratio). `preview` uses the coarse base map (for
/// interactive globe rotation).
pub fn render_options(
    map: &ComputedMap,
    settings: &MapSettings,
    width: u32,
    preview: bool,
) -> RenderOptions {
    let scale = (width / 1400).max(1);
    let markers = if settings.cities {
        CITIES
            .iter()
            .map(|&(name, lat, lon)| Marker {
                radius_px: 3.0 * scale as f32,
                ..Marker::new(lat, lon, name)
            })
            .collect()
    } else {
        Vec::new()
    };
    let px = scale as f32;
    let defaults = RenderOptions::default();
    RenderOptions {
        projection: settings.projection(),
        width: width.max(16),
        basemap: if preview { Scale::Ne110m } else { Scale::Ne50m },
        zone_opacity: settings.zone_opacity.clamp(0.0, 1.0),
        smoothing: ZoneSmoothing::Smooth,
        coastlines: Some(LineStyle::new([60, 60, 70, 255], 0.8 * px)),
        borders: settings
            .borders
            .then(|| LineStyle::new([110, 110, 120, 200], 0.5 * px)),
        graticule: settings.graticule.then_some(Graticule {
            spacing_deg: 30.0,
            style: LineStyle::new([90, 90, 110, 90], 0.6 * px),
        }),
        frame: Some(LineStyle::new([60, 60, 70, 255], px)),
        markers,
        label_scale: scale,
        legend: settings.legend_in_image.then(|| legend(map, scale, true)),
        ..defaults
    }
    .fit_height()
}

/// The data to draw (zone palette and, when enabled and available, the
/// continuous value field for true contours).
pub fn map_data<'a>(
    map: &'a ComputedMap,
    settings: &MapSettings,
    palette: &'a [[u8; 3]],
) -> MapData<'a> {
    MapData {
        grid: &map.grid,
        palette,
        none_color: None,
        field: if settings.contours {
            map.field.zone_field()
        } else {
            None
        },
    }
}

pub fn palette(map: &ComputedMap) -> Vec<[u8; 3]> {
    map.zones.iter().map(|z| z.rgb).collect()
}

/// Render the interactive map image.
pub fn render_view(
    map: &ComputedMap,
    settings: &MapSettings,
    width: u32,
    preview: bool,
) -> RgbaImage {
    let pal = palette(map);
    let opts = render_options(map, settings, width, preview);
    render_raster(&map_data(map, settings, &pal), &opts)
}

/// Options for an exported map.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportOptions {
    pub width: u32,
    pub legend: bool,
    pub title: bool,
}

fn export_render_options(
    map: &ComputedMap,
    settings: &MapSettings,
    o: &ExportOptions,
) -> RenderOptions {
    let mut opts = render_options(map, settings, o.width, false);
    let scale = (o.width / 1000).max(1);
    opts.legend = o.legend.then(|| legend(map, scale, true));
    opts
}

fn title_band(o: &ExportOptions) -> (u32, u32) {
    let scale = (o.width / 1000).max(1);
    (if o.title { 7 * scale + 16 } else { 0 }, scale)
}

/// The exported raster: an optional title band above the map, legend in the
/// map's lower-left corner.
pub fn export_raster(map: &ComputedMap, settings: &MapSettings, o: &ExportOptions) -> RgbaImage {
    let pal = palette(map);
    let mut opts = export_render_options(map, settings, o);
    // A white page behind the globe and outside the Robinson outline.
    opts.background = [255, 255, 255, 255];
    let img = render_raster(&map_data(map, settings, &pal), &opts);
    let (band, scale) = title_band(o);
    if band == 0 {
        return img;
    }
    let mut canvas = RgbaImage::new(img.width, img.height + band, [255, 255, 255, 255]);
    let row = img.width as usize * 4;
    canvas.pixels[band as usize * row..].copy_from_slice(&img.pixels);
    let title = Legend {
        title: Some(ascii_label(&map_title(map))),
        entries: Vec::new(),
        position: LegendPosition::TopLeft,
        text_scale: scale,
        background: [255, 255, 255, 0],
        border_color: None,
        ..Legend::default()
    };
    mooncalc_maps::render::draw_legend(&mut canvas, &title);
    canvas
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The exported SVG: zones from the grid cells, an optional title above the
/// map and the legend.
pub fn export_svg(map: &ComputedMap, settings: &MapSettings, o: &ExportOptions) -> String {
    let pal = palette(map);
    let mut opts = export_render_options(map, settings, o);
    opts.background = [255, 255, 255, 255];
    let data = MapData {
        field: None,
        ..map_data(map, settings, &pal)
    };
    let inner = render_svg(&data, &opts);
    let (band, scale) = title_band(o);
    let title = map_title(map);
    let body = inner
        .strip_prefix(r#"<?xml version="1.0" encoding="UTF-8"?>"#)
        .unwrap_or(&inner);
    let body = body.replacen("<svg ", &format!(r#"<svg y="{band}" "#), 1);
    let (w, h) = (opts.width, opts.height + band);
    let mut out = format!(
        r##"<?xml version="1.0" encoding="UTF-8"?><svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><title>{}</title><rect width="{w}" height="{h}" fill="#ffffff"/>"##,
        xml_escape(&title)
    );
    if band > 0 {
        out.push_str(&format!(
            r##"<text x="8" y="{}" font-family="sans-serif" font-size="{}" fill="#141414">{}</text>"##,
            band - 6 * scale,
            12 * scale,
            xml_escape(&title)
        ));
    }
    out.push_str(&body);
    out.push_str("</svg>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_labels_have_no_unsupported_glyphs() {
        assert_eq!(
            ascii_label("Easily visible (ARCL ≥ 12°) — W′"),
            "Easily visible (ARCL >= 12°) - W'"
        );
        assert_eq!(ascii_label("日"), "?");
    }

    #[test]
    fn projections_follow_the_settings() {
        let mut s = MapSettings::default();
        assert!(matches!(s.projection(), MapProjection::Equirectangular(_)));
        s.projection = ProjectionKind::Globe;
        assert_eq!(
            s.projection(),
            MapProjection::Orthographic(Orthographic::new(s.globe_lat_deg, s.globe_lon_deg))
        );
        s.projection = ProjectionKind::Robinson;
        s.center_lon_deg = 150.0;
        assert_eq!(
            s.projection(),
            MapProjection::Robinson(Robinson::new(150.0))
        );
    }
}
