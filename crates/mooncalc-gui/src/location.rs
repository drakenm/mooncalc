//! Observer-location helpers: coordinate formatting and the city search hook.

/// A city offered by the search box.
#[derive(Clone, Debug, PartialEq)]
pub struct CitySuggestion {
    pub name: String,
    /// Country (and region, if useful), e.g. "Saudi Arabia".
    pub country: String,
    pub lat_deg: f64,
    /// East-positive.
    pub lon_deg: f64,
    pub elevation_m: f64,
    /// IANA time-zone offset in hours at the selected date, if known.
    pub utc_offset_hours: Option<f64>,
}

impl CitySuggestion {
    /// Text shown in the result list.
    pub fn display(&self) -> String {
        if self.country.is_empty() {
            format!(
                "{} ({})",
                self.name,
                format_lat_lon(self.lat_deg, self.lon_deg)
            )
        } else {
            format!(
                "{}, {} ({})",
                self.name,
                self.country,
                format_lat_lon(self.lat_deg, self.lon_deg)
            )
        }
    }
}

/// **Extension point.** Search the city atlas for `query`, best match
/// first, at most `limit` results.
///
/// Wire this to `mooncalc_atlas` (embedded GeoNames data) once the atlas
/// package lands. Until then it returns no results, and the search box tells
/// the user the atlas is not available yet ([`atlas_available`]).
pub fn search_cities(query: &str, limit: usize) -> Vec<CitySuggestion> {
    let _ = (query, limit);
    Vec::new()
}

/// Whether [`search_cities`] is backed by real data yet.
pub fn atlas_available() -> bool {
    false
}

/// Wrap a longitude into `(-180, 180]`.
pub fn normalize_lon(lon_deg: f64) -> f64 {
    let l = (lon_deg + 180.0).rem_euclid(360.0) - 180.0;
    if l == -180.0 { 180.0 } else { l }
}

/// `21.4225° N`.
pub fn format_lat(lat_deg: f64) -> String {
    let hemi = if lat_deg < 0.0 { 'S' } else { 'N' };
    format!("{:.4}° {hemi}", lat_deg.abs())
}

/// `39.8262° E`.
pub fn format_lon(lon_deg: f64) -> String {
    let lon = normalize_lon(lon_deg);
    let hemi = if lon < 0.0 { 'W' } else { 'E' };
    format!("{:.4}° {hemi}", lon.abs())
}

/// `21.4225° N, 39.8262° E`.
pub fn format_lat_lon(lat_deg: f64, lon_deg: f64) -> String {
    format!("{}, {}", format_lat(lat_deg), format_lon(lon_deg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_hemispheres() {
        assert_eq!(format_lat_lon(21.4225, 39.8262), "21.4225° N, 39.8262° E");
        assert_eq!(format_lat_lon(-33.9249, -70.6693), "33.9249° S, 70.6693° W");
        assert_eq!(format_lon(190.0), "170.0000° W");
    }

    #[test]
    fn normalizes_longitude() {
        assert_eq!(normalize_lon(-180.0), 180.0);
        assert_eq!(normalize_lon(540.0), 180.0);
        assert!((normalize_lon(-190.0) - 170.0).abs() < 1e-12);
        assert!((normalize_lon(359.5) + 0.5).abs() < 1e-12);
    }
}
