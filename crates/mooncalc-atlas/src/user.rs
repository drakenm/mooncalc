//! User-defined locations (MoonCalc 6 allowed up to 1000 of them).
//!
//! The core API works on JSON strings so it is usable on every target,
//! including `wasm32-unknown-unknown`; file helpers are thin native-only
//! wrappers.

use std::fmt;

use mooncalc_core::GeoLocation;
use serde::{Deserialize, Serialize};

use crate::atlas::{Atlas, City, MatchQuality, Query, match_keys};
use crate::fold::fold;

/// Maximum number of user locations, as in MoonCalc 6.
pub const MAX_USER_LOCATIONS: usize = 1000;

/// Current JSON format version.
const FORMAT_VERSION: u32 = 1;

/// A location defined by the user.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UserLocation {
    pub name: String,
    pub lat_deg: f64,
    /// East-positive.
    pub lon_deg: f64,
    #[serde(default)]
    pub elevation_m: f64,
    /// Fixed UTC offset used for local times, hours (east-positive), when
    /// there is no `time_zone`.
    #[serde(default)]
    pub utc_offset_hours: f64,
    /// IANA time zone for local times (with its DST rules), e.g.
    /// `Europe/London`. Absent in files saved before zones were supported;
    /// an unknown name falls back to `utc_offset_hours` ([`Self::zone`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_zone: Option<String>,
}

impl UserLocation {
    pub fn new(
        name: impl Into<String>,
        lat_deg: f64,
        lon_deg: f64,
        elevation_m: f64,
        utc_offset_hours: f64,
    ) -> Self {
        Self {
            name: name.into(),
            lat_deg,
            lon_deg,
            elevation_m,
            utc_offset_hours,
            time_zone: None,
        }
    }

    /// The same location with an IANA time zone.
    pub fn with_time_zone(mut self, time_zone: Option<String>) -> Self {
        self.time_zone = time_zone.filter(|z| !z.trim().is_empty());
        self
    }

    /// The zone for local times: the IANA zone when set and known, else the
    /// fixed offset.
    pub fn zone(&self) -> crate::Zone {
        crate::Zone::iana_or_fixed(self.time_zone.as_deref(), self.utc_offset_hours)
    }

    pub fn location(&self) -> GeoLocation {
        GeoLocation::new(self.lat_deg, self.lon_deg, self.elevation_m)
    }

    /// Checks the fields are usable: a non-empty name, latitude in
    /// [-90, 90], longitude in [-180, 180], elevation within
    /// [-500, 9000] m and UTC offset in [-14, 14] h.
    pub fn validate(&self) -> Result<(), UserAtlasError> {
        let bad = |what: &str| Err(UserAtlasError::Invalid(format!("{}: {what}", self.name)));
        if self.name.trim().is_empty() {
            return Err(UserAtlasError::Invalid("empty name".into()));
        }
        if !(-90.0..=90.0).contains(&self.lat_deg) {
            return bad("latitude out of range");
        }
        if !(-180.0..=180.0).contains(&self.lon_deg) {
            return bad("longitude out of range");
        }
        if !(-500.0..=9000.0).contains(&self.elevation_m) {
            return bad("elevation out of range");
        }
        if !(-14.0..=14.0).contains(&self.utc_offset_hours) {
            return bad("UTC offset out of range");
        }
        Ok(())
    }
}

/// Errors from the user atlas.
#[derive(Debug)]
pub enum UserAtlasError {
    /// Already holding [`MAX_USER_LOCATIONS`] entries.
    Full,
    /// A location failed [`UserLocation::validate`].
    Invalid(String),
    /// Malformed JSON or an unsupported format version.
    Json(String),
    /// File read/write failure (native targets only).
    Io(std::io::Error),
}

impl fmt::Display for UserAtlasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full => write!(f, "the user atlas is full ({MAX_USER_LOCATIONS} locations)"),
            Self::Invalid(m) => write!(f, "invalid location: {m}"),
            Self::Json(m) => write!(f, "invalid user atlas JSON: {m}"),
            Self::Io(e) => write!(f, "user atlas I/O error: {e}"),
        }
    }
}

impl std::error::Error for UserAtlasError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct FileFormat {
    version: u32,
    locations: Vec<UserLocation>,
}

/// The user's own locations, in insertion order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UserAtlas {
    locations: Vec<UserLocation>,
}

impl UserAtlas {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn locations(&self) -> &[UserLocation] {
        &self.locations
    }

    pub fn len(&self) -> usize {
        self.locations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.locations.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&UserLocation> {
        self.locations.get(index)
    }

    /// Adds a location, returning its index.
    pub fn add(&mut self, loc: UserLocation) -> Result<usize, UserAtlasError> {
        if self.locations.len() >= MAX_USER_LOCATIONS {
            return Err(UserAtlasError::Full);
        }
        loc.validate()?;
        self.locations.push(loc);
        Ok(self.locations.len() - 1)
    }

    /// Replaces the location at `index`. Returns `Invalid` for a bad index.
    pub fn replace(&mut self, index: usize, loc: UserLocation) -> Result<(), UserAtlasError> {
        loc.validate()?;
        let slot = self
            .locations
            .get_mut(index)
            .ok_or_else(|| UserAtlasError::Invalid(format!("no location at index {index}")))?;
        *slot = loc;
        Ok(())
    }

    /// Removes and returns the location at `index`.
    pub fn remove(&mut self, index: usize) -> Option<UserLocation> {
        (index < self.locations.len()).then(|| self.locations.remove(index))
    }

    /// Parses the JSON format written by [`UserAtlas::to_json_string`]:
    /// `{"version": 1, "locations": [{"name", "lat_deg", "lon_deg",
    /// "elevation_m", "utc_offset_hours"}, ...]}`. Every entry is validated.
    pub fn from_json_str(json: &str) -> Result<Self, UserAtlasError> {
        let file: FileFormat =
            serde_json::from_str(json).map_err(|e| UserAtlasError::Json(e.to_string()))?;
        if file.version != FORMAT_VERSION {
            return Err(UserAtlasError::Json(format!(
                "unsupported version {}",
                file.version
            )));
        }
        let mut atlas = UserAtlas::new();
        for loc in file.locations {
            atlas.add(loc)?;
        }
        Ok(atlas)
    }

    /// Serialises to pretty-printed JSON.
    pub fn to_json_string(&self) -> String {
        let file = FileFormat {
            version: FORMAT_VERSION,
            locations: self.locations.clone(),
        };
        serde_json::to_string_pretty(&file).expect("user atlas serialises")
    }

    /// Loads from a file; a missing file gives an empty atlas.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Self, UserAtlasError> {
        match std::fs::read_to_string(path) {
            Ok(s) => Self::from_json_str(&s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::new()),
            Err(e) => Err(UserAtlasError::Io(e)),
        }
    }

    /// Saves to a file (written to a temporary sibling, then renamed).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save(&self, path: impl AsRef<std::path::Path>) -> Result<(), UserAtlasError> {
        let path = path.as_ref();
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        std::fs::write(&tmp, self.to_json_string()).map_err(UserAtlasError::Io)?;
        std::fs::rename(&tmp, path).map_err(UserAtlasError::Io)
    }

    /// Searches user locations by name with the same matching rules as
    /// [`Atlas::search`] (qualifiers after a comma are ignored).
    pub fn search(&self, query: &str, limit: usize) -> Vec<&UserLocation> {
        let Some(q) = Query::new(query) else {
            return Vec::new();
        };
        let mut hits: Vec<(MatchQuality, usize)> = self
            .locations
            .iter()
            .enumerate()
            .filter_map(|(i, l)| match_keys(&fold(&l.name), "", &q.name).map(|m| (m, i)))
            .collect();
        hits.sort_by_key(|&(m, i)| (m, i));
        hits.truncate(limit);
        hits.into_iter().map(|(_, i)| &self.locations[i]).collect()
    }
}

/// A search result from either the user atlas or the built-in atlas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Place<'a> {
    User(&'a UserLocation),
    City(&'a City),
}

impl Place<'_> {
    pub fn name(&self) -> &str {
        match self {
            Place::User(u) => &u.name,
            Place::City(c) => c.name,
        }
    }

    /// A label for lists: the user name, or `City, Admin1, Country`.
    pub fn display_name(&self) -> String {
        match self {
            Place::User(u) => u.name.clone(),
            Place::City(c) => c.display_name(),
        }
    }

    pub fn location(&self) -> GeoLocation {
        match self {
            Place::User(u) => u.location(),
            Place::City(c) => c.location(),
        }
    }

    /// Standard-time UTC offset, hours.
    pub fn utc_offset_hours(&self) -> f64 {
        match self {
            Place::User(u) => u.utc_offset_hours,
            Place::City(c) => c.std_offset_hours,
        }
    }
}

impl Atlas {
    /// Searches the user atlas and this atlas together: user matches come
    /// first, then cities, up to `limit` results in total.
    pub fn search_with_user<'a>(
        &'a self,
        user: &'a UserAtlas,
        query: &str,
        limit: usize,
    ) -> Vec<Place<'a>> {
        let mut out: Vec<Place<'a>> = user
            .search(query, limit)
            .into_iter()
            .map(Place::User)
            .collect();
        let rest = limit - out.len();
        if rest > 0 {
            out.extend(self.search(query, rest).into_iter().map(Place::City));
        }
        out
    }
}

#[cfg(test)]
mod zone_tests {
    use super::*;

    #[test]
    fn time_zone_is_optional_and_round_trips() {
        let old = r#"{"version":1,"locations":[{"name":"Home","lat_deg":51.5,"lon_deg":0.0,"utc_offset_hours":1.0}]}"#;
        let a = UserAtlas::from_json_str(old).unwrap();
        let home = a.get(0).unwrap();
        assert_eq!(home.time_zone, None);
        assert_eq!(home.zone(), crate::Zone::fixed_hours(1.0));

        let mut b = UserAtlas::new();
        let loc = UserLocation::new("Office", 40.7, -74.0, 10.0, -5.0)
            .with_time_zone(Some("America/New_York".into()));
        b.add(loc).unwrap();
        let json = b.to_json_string();
        assert!(json.contains("America/New_York"));
        let back = UserAtlas::from_json_str(&json).unwrap();
        assert_eq!(
            back.get(0).unwrap().zone().iana_name(),
            Some("America/New_York")
        );
        // No key is written without a zone, so older builds read the file.
        assert!(!a.to_json_string().contains("time_zone"));
        // An unknown zone falls back to the fixed offset.
        let odd =
            UserLocation::new("X", 0.0, 0.0, 0.0, 2.0).with_time_zone(Some("Mars/Base".into()));
        assert_eq!(odd.zone(), crate::Zone::fixed_hours(2.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> UserAtlas {
        let mut u = UserAtlas::new();
        u.add(UserLocation::new(
            "Home – Leicester",
            52.6369,
            -1.1398,
            60.0,
            0.0,
        ))
        .unwrap();
        u.add(UserLocation::new(
            "Observatory hill",
            -33.85,
            151.2,
            43.5,
            10.0,
        ))
        .unwrap();
        u.add(UserLocation::new(
            "Londonderry field",
            54.99,
            -7.32,
            8.0,
            0.0,
        ))
        .unwrap();
        u
    }

    #[test]
    fn json_round_trip() {
        let u = sample();
        let json = u.to_json_string();
        assert!(json.contains("\"version\": 1"));
        let back = UserAtlas::from_json_str(&json).unwrap();
        assert_eq!(back, u);
        assert_eq!(
            UserAtlas::from_json_str(&UserAtlas::new().to_json_string())
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn json_defaults_and_errors() {
        let u = UserAtlas::from_json_str(
            r#"{"version":1,"locations":[{"name":"X","lat_deg":1,"lon_deg":2}]}"#,
        )
        .unwrap();
        assert_eq!(u.get(0).unwrap().elevation_m, 0.0);
        assert!(matches!(
            UserAtlas::from_json_str(r#"{"version":2,"locations":[]}"#),
            Err(UserAtlasError::Json(_))
        ));
        assert!(matches!(
            UserAtlas::from_json_str("not json"),
            Err(UserAtlasError::Json(_))
        ));
        assert!(matches!(
            UserAtlas::from_json_str(
                r#"{"version":1,"locations":[{"name":"X","lat_deg":100,"lon_deg":2}]}"#
            ),
            Err(UserAtlasError::Invalid(_))
        ));
    }

    #[test]
    fn capacity_and_validation() {
        let mut u = UserAtlas::new();
        for i in 0..MAX_USER_LOCATIONS {
            u.add(UserLocation::new(format!("P{i}"), 0.0, 0.0, 0.0, 0.0))
                .unwrap();
        }
        assert!(matches!(
            u.add(UserLocation::new("one more", 0.0, 0.0, 0.0, 0.0)),
            Err(UserAtlasError::Full)
        ));
        let mut u = UserAtlas::new();
        assert!(u.add(UserLocation::new(" ", 0.0, 0.0, 0.0, 0.0)).is_err());
        assert!(u.add(UserLocation::new("x", 0.0, 181.0, 0.0, 0.0)).is_err());
        assert!(
            u.add(UserLocation::new("x", f64::NAN, 0.0, 0.0, 0.0))
                .is_err()
        );
        assert!(u.add(UserLocation::new("x", 0.0, 0.0, 0.0, 15.0)).is_err());
        let i = u.add(UserLocation::new("x", 0.0, 0.0, 0.0, 5.5)).unwrap();
        u.replace(i, UserLocation::new("y", 1.0, 1.0, 1.0, 1.0))
            .unwrap();
        assert_eq!(u.get(i).unwrap().name, "y");
        assert!(
            u.replace(9, UserLocation::new("z", 0.0, 0.0, 0.0, 0.0))
                .is_err()
        );
        assert_eq!(u.remove(i).unwrap().name, "y");
        assert!(u.remove(0).is_none());
    }

    #[test]
    fn merged_search_puts_user_first() {
        let u = sample();
        let hits = Atlas::builtin().search_with_user(&u, "london", 5);
        assert_eq!(hits.len(), 5);
        assert_eq!(hits[0], Place::User(u.get(2).unwrap()));
        assert_eq!(hits[1].name(), "London");
        assert!(matches!(hits[1], Place::City(c) if c.country_code == "GB"));
        // Diacritic/case-insensitive user search, by word prefix.
        assert_eq!(u.search("LEICESTER", 5)[0].name, "Home – Leicester");
        assert_eq!(u.search("hill", 5)[0].utc_offset_hours, 10.0);
        let only_user = Atlas::builtin().search_with_user(&u, "observatory", 1);
        assert_eq!(only_user.len(), 1);
        assert_eq!(only_user[0].utc_offset_hours(), 10.0);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn file_round_trip() {
        let dir = std::env::temp_dir().join(format!("mooncalc-atlas-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("user_atlas.json");
        let _ = std::fs::remove_file(&path);
        assert!(UserAtlas::load(&path).unwrap().is_empty());
        let u = sample();
        u.save(&path).unwrap();
        assert_eq!(UserAtlas::load(&path).unwrap(), u);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
