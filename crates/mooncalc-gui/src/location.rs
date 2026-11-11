//! Observer-location helpers: coordinate formatting, the city atlas
//! (search, nearest city, background warm-up) and the user's own saved
//! locations.
//!
//! The atlas is [`mooncalc_atlas::Atlas::builtin`] (GeoNames
//! `cities15000`). Parsing it takes about 70 ms in a release build and
//! about 0.8 s in a debug build, so the app calls [`warm_atlas`] at startup
//! and [`atlas`] returns `None` until the parse has finished; the location
//! panel shows "loading atlas…" meanwhile.
//!
//! User locations are a [`UserAtlas`], persisted as its JSON string under
//! [`USER_ATLAS_KEY`] in eframe storage (see [`load_user_atlas`]).

use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};

use mooncalc_atlas::{Atlas, Place, UserAtlas, UserAtlasError, UserLocation, Zone};

use crate::settings::{LocationSettings, TimeSettings};

/// eframe storage key for the user-locations JSON
/// ([`UserAtlas::to_json_string`]).
pub const USER_ATLAS_KEY: &str = "mooncalc.user_atlas";

/// Short credit shown next to atlas data (the full text is in
/// [`mooncalc_atlas::ATTRIBUTION`], on the About tab).
pub const ATLAS_CREDIT: &str = "City data © GeoNames, CC BY 4.0";

/// A place offered by the search box: a built-in city or a user location.
#[derive(Clone, Debug, PartialEq)]
pub struct CitySuggestion {
    pub name: String,
    /// `City, Admin1, Country` for a city; the name for a user location.
    pub display_name: String,
    /// Country name; empty for a user location.
    pub country: String,
    pub lat_deg: f64,
    /// East-positive.
    pub lon_deg: f64,
    pub elevation_m: f64,
    /// Population, for built-in cities.
    pub population: Option<u64>,
    /// IANA time-zone identifier: always for built-in cities, for user
    /// locations when saved with one.
    pub timezone: Option<String>,
    /// Standard-time UTC offset, hours (a user location's saved offset).
    pub std_offset_hours: f64,
    /// Daylight-saving UTC offset, hours; equals `std_offset_hours` where
    /// DST is not observed (and for user locations).
    pub dst_offset_hours: f64,
    /// Whether this is one of the user's own locations.
    pub is_user: bool,
}

impl CitySuggestion {
    /// Converts an atlas search result.
    pub fn from_place(place: Place<'_>) -> Self {
        match place {
            Place::User(u) => Self::from_user(u),
            Place::City(c) => Self {
                name: c.name.to_owned(),
                display_name: c.display_name(),
                country: c.country_name.to_owned(),
                lat_deg: c.lat_deg,
                lon_deg: c.lon_deg,
                elevation_m: c.elevation_m,
                population: Some(c.population),
                timezone: Some(c.timezone.to_owned()),
                std_offset_hours: c.std_offset_hours,
                dst_offset_hours: c.dst_offset_hours,
                is_user: false,
            },
        }
    }

    /// Converts a user location.
    pub fn from_user(u: &UserLocation) -> Self {
        Self {
            name: u.name.clone(),
            display_name: u.name.clone(),
            country: String::new(),
            lat_deg: u.lat_deg,
            lon_deg: u.lon_deg,
            elevation_m: u.elevation_m,
            population: None,
            timezone: u.time_zone.clone(),
            std_offset_hours: u.utc_offset_hours,
            dst_offset_hours: u.utc_offset_hours,
            is_user: true,
        }
    }

    /// Whether the place's time zone observes daylight-saving time.
    pub fn observes_dst(&self) -> bool {
        self.dst_offset_hours != self.std_offset_hours
    }

    /// Sets the time zone for local times: the place's IANA zone (whose
    /// rules give the offset on every date) or, for a user location saved
    /// without one, its fixed offset.
    pub fn apply_zone_to(&self, time: &mut TimeSettings) {
        match self.timezone.as_deref().filter(|z| Zone::iana(z).is_ok()) {
            Some(z) => time.set_zone(z),
            None => time.set_fixed(self.std_offset_hours),
        }
    }

    /// Text shown in the result list: name on the first line, then
    /// coordinates, population and time-zone offset.
    pub fn display(&self) -> String {
        let mut details = format_lat_lon(self.lat_deg, self.lon_deg);
        if let Some(p) = self.population {
            details.push_str(&format!(" · pop. {}", format_population(p)));
        }
        details.push_str(&format!(
            " · {}",
            crate::datetime::format_utc_offset(self.std_offset_hours)
        ));
        if self.observes_dst() {
            details.push_str(&format!(
                " ({} DST)",
                crate::datetime::format_utc_offset(self.dst_offset_hours)
            ));
        }
        let tag = if self.is_user { " ★" } else { "" };
        format!("{}{tag}\n{details}", self.display_name)
    }

    /// Writes the place into the observer location.
    pub fn apply_to(&self, loc: &mut LocationSettings) {
        loc.name = self.name.clone();
        loc.lat_deg = self.lat_deg;
        loc.lon_deg = self.lon_deg;
        loc.elevation_m = self.elevation_m;
    }
}

/// `1 234 567` → `1.23 M`, `45 600` → `45.6 k`.
pub fn format_population(p: u64) -> String {
    match p {
        0..=999 => p.to_string(),
        1_000..=999_999 => format!("{:.1} k", p as f64 / 1e3),
        _ => format!("{:.2} M", p as f64 / 1e6),
    }
}

static ATLAS_READY: AtomicBool = AtomicBool::new(false);
static WARM: Once = Once::new();

/// Start parsing the built-in atlas on a background thread (once per
/// process). `on_ready` runs on that thread when it is done, e.g. to
/// request a repaint.
pub fn warm_atlas(on_ready: impl FnOnce() + Send + 'static) {
    WARM.call_once(|| {
        let spawned = std::thread::Builder::new()
            .name("atlas-warmup".into())
            .spawn(move || {
                let _ = Atlas::builtin();
                ATLAS_READY.store(true, Ordering::Release);
                on_ready();
            });
        if spawned.is_err() {
            // No threads: parse inline rather than never.
            let _ = Atlas::builtin();
            ATLAS_READY.store(true, Ordering::Release);
        }
    });
}

/// The built-in atlas, or `None` while [`warm_atlas`] is still parsing it
/// (or was never called). Never blocks the UI thread.
pub fn atlas() -> Option<&'static Atlas> {
    ATLAS_READY.load(Ordering::Acquire).then(Atlas::builtin)
}

/// Whether [`search_cities`] is backed by real data (the GeoNames atlas is
/// built in; it may still be loading, see [`atlas`]).
pub fn atlas_available() -> bool {
    true
}

/// Search the user's locations and the city atlas for `query`, user
/// matches first, then cities by match quality and population; at most
/// `limit` results. `None` while the atlas is still loading.
pub fn search_cities(user: &UserAtlas, query: &str, limit: usize) -> Option<Vec<CitySuggestion>> {
    atlas().map(|a| search_in(a, user, query, limit))
}

/// [`search_cities`] against a given atlas.
pub fn search_in(
    atlas: &Atlas,
    user: &UserAtlas,
    query: &str,
    limit: usize,
) -> Vec<CitySuggestion> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }
    atlas
        .search_with_user(user, query, limit)
        .into_iter()
        .map(CitySuggestion::from_place)
        .collect()
}

/// The atlas city nearest to a point and its distance in km. `None` while
/// the atlas is loading.
pub fn nearest_city(lat_deg: f64, lon_deg: f64) -> Option<(CitySuggestion, f64)> {
    let (city, km) = atlas()?.nearest_km(lat_deg, lon_deg)?;
    Some((CitySuggestion::from_place(Place::City(city)), km))
}

/// Restore the user atlas from its stored JSON. A missing value gives an
/// empty atlas; malformed JSON gives an empty atlas and the error (the
/// caller reports it, and the stored value is overwritten on the next save).
pub fn load_user_atlas(json: Option<&str>) -> (UserAtlas, Option<UserAtlasError>) {
    match json {
        None => (UserAtlas::new(), None),
        Some(s) if s.trim().is_empty() => (UserAtlas::new(), None),
        Some(s) => match UserAtlas::from_json_str(s) {
            Ok(a) => (a, None),
            Err(e) => (UserAtlas::new(), Some(e)),
        },
    }
}

/// A user location for the current observer and time zone (the IANA zone
/// in zone mode, with the offset at the selected time as its fixed offset).
pub fn user_location_from(name: &str, loc: &LocationSettings, time: &TimeSettings) -> UserLocation {
    let zone = time.zone();
    UserLocation::new(
        name.trim(),
        loc.lat_deg,
        normalize_lon(loc.lon_deg),
        loc.elevation_m,
        zone.offset_hours(time.jd_ut),
    )
    .with_time_zone(zone.iana_name().map(str::to_owned))
}

/// Rename the user location at `index`.
pub fn rename_user_location(
    user: &mut UserAtlas,
    index: usize,
    new_name: &str,
) -> Result<(), UserAtlasError> {
    let mut loc = user
        .get(index)
        .cloned()
        .ok_or_else(|| UserAtlasError::Invalid(format!("no location at index {index}")))?;
    loc.name = new_name.trim().to_owned();
    user.replace(index, loc)
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

    #[test]
    fn formats_population() {
        assert_eq!(format_population(950), "950");
        assert_eq!(format_population(45_600), "45.6 k");
        assert_eq!(format_population(1_234_567), "1.23 M");
    }

    #[test]
    fn search_results_map_city_fields() {
        let atlas = Atlas::builtin();
        let user = UserAtlas::new();
        let hits = search_in(atlas, &user, "Mecca", 5);
        let mecca = hits.first().expect("Mecca is in the atlas");
        assert!(!mecca.is_user);
        assert_eq!(mecca.country, "Saudi Arabia");
        assert!(mecca.display_name.contains("Saudi Arabia"));
        assert!((mecca.lat_deg - 21.42).abs() < 0.1, "{}", mecca.lat_deg);
        assert!((mecca.lon_deg - 39.83).abs() < 0.1, "{}", mecca.lon_deg);
        assert!(mecca.population.unwrap() > 1_000_000);
        assert_eq!(mecca.timezone.as_deref(), Some("Asia/Riyadh"));
        assert_eq!(mecca.std_offset_hours, 3.0);
        assert!(!mecca.observes_dst());
        assert_eq!(mecca.dst_offset_hours, 3.0);
        assert!(mecca.display().contains("pop."));

        let mut loc = LocationSettings::default();
        loc.name.clear();
        mecca.apply_to(&mut loc);
        assert_eq!(loc.name, mecca.name);
        assert_eq!(loc.lat_deg, mecca.lat_deg);
        assert_eq!(loc.elevation_m, mecca.elevation_m);

        assert!(search_in(atlas, &user, "   ", 5).is_empty());
    }

    /// Time settings with a fixed offset.
    fn fixed(hours: f64) -> TimeSettings {
        let mut t = TimeSettings::default();
        t.set_fixed(hours);
        t
    }

    #[test]
    fn choosing_a_city_selects_its_zone() {
        let hits = search_in(Atlas::builtin(), &UserAtlas::new(), "London, GB", 1);
        let london = &hits[0];
        assert!(london.observes_dst());
        assert!(london.display().contains("DST"));
        let mut time = fixed(3.0);
        // 2024-07-01 12:00 UT: BST.
        time.jd_ut = 2_460_493.0;
        london.apply_zone_to(&mut time);
        assert_eq!(time.tz_mode, crate::settings::TzMode::Zone);
        assert_eq!(time.zone().iana_name(), Some("Europe/London"));
        assert_eq!(time.offset_hours(), 1.0);
        time.jd_ut -= 180.0;
        assert_eq!(time.offset_hours(), 0.0);
    }

    #[test]
    fn user_locations_come_first_and_round_trip() {
        let mut user = UserAtlas::new();
        let loc = LocationSettings {
            name: "ignored".into(),
            lat_deg: 51.5,
            lon_deg: 359.9, // normalised on save
            elevation_m: 35.0,
        };
        user.add(user_location_from("  London rooftop ", &loc, &fixed(1.0)))
            .unwrap();
        user.add(user_location_from("Hilltop", &loc, &fixed(0.0)))
            .unwrap();
        let mut zoned = fixed(0.0);
        zoned.set_zone("Europe/London");
        user.add(user_location_from("Office", &loc, &zoned))
            .unwrap();
        assert_eq!(
            user.get(2).unwrap().time_zone.as_deref(),
            Some("Europe/London")
        );

        // Round trip through the stored JSON string.
        let json = user.to_json_string();
        let (back, err) = load_user_atlas(Some(&json));
        assert!(err.is_none());
        assert_eq!(back, user);
        assert_eq!(back.get(0).unwrap().name, "London rooftop");
        assert!((back.get(0).unwrap().lon_deg + 0.1).abs() < 1e-9);

        let hits = search_in(Atlas::builtin(), &back, "London", 4);
        assert!(hits[0].is_user);
        assert_eq!(hits[0].name, "London rooftop");
        assert_eq!(hits[0].std_offset_hours, 1.0);
        assert!(!hits[1].is_user);

        // Rename and delete.
        let mut edited = back.clone();
        rename_user_location(&mut edited, 1, "Valley").unwrap();
        assert_eq!(edited.get(1).unwrap().name, "Valley");
        assert!(rename_user_location(&mut edited, 1, "  ").is_err());
        assert!(rename_user_location(&mut edited, 9, "x").is_err());
        edited.remove(0);
        assert_eq!(edited.len(), 2);
        let (again, _) = load_user_atlas(Some(&edited.to_json_string()));
        assert_eq!(again.get(0).unwrap().name, "Valley");
    }

    #[test]
    fn bad_or_missing_user_json_gives_empty_atlas() {
        let (a, e) = load_user_atlas(None);
        assert!(a.is_empty() && e.is_none());
        let (a, e) = load_user_atlas(Some(""));
        assert!(a.is_empty() && e.is_none());
        let (a, e) = load_user_atlas(Some("{not json"));
        assert!(a.is_empty() && e.is_some());
    }

    #[test]
    fn warm_up_makes_the_atlas_available() {
        let (tx, rx) = std::sync::mpsc::channel();
        warm_atlas(move || {
            let _ = tx.send(());
        });
        // Another test may already have warmed it (then the callback never
        // runs); either way the atlas becomes available.
        let _ = rx.recv_timeout(std::time::Duration::from_secs(30));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while atlas().is_none() && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        let (city, km) = nearest_city(21.4225, 39.8262).expect("atlas loaded");
        assert!(km < 20.0, "{} at {km} km", city.name);
        assert!(search_cities(&UserAtlas::new(), "Cairo", 3).is_some());
    }
}
