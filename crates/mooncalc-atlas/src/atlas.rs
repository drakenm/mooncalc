//! The embedded GeoNames city atlas: parsing, search and nearest-city lookup.

use std::fmt;
use std::sync::OnceLock;

use mooncalc_core::GeoLocation;

use crate::fold::{fold, word_prefix};

/// The embedded atlas file, produced by `tools/atlas/build_atlas.py`.
static BUILTIN_TSV: &str = include_str!("../../../assets/atlas/cities.tsv");

/// Mean Earth radius (IUGG), km.
const EARTH_RADIUS_KM: f64 = 6371.0088;

/// Size of a spatial-index cell, degrees.
const CELL_DEG: f64 = 2.0;
const GRID_LAT: usize = (180.0 / CELL_DEG) as usize;
const GRID_LON: usize = (360.0 / CELL_DEG) as usize;

/// A populated place from the GeoNames `cities15000` dump.
#[derive(Clone, Debug, PartialEq)]
pub struct City {
    /// Name in its local Latin-script form, e.g. `São Paulo`.
    pub name: &'static str,
    /// ASCII transliteration, e.g. `Sao Paulo`.
    pub ascii_name: &'static str,
    /// `|`-separated alternate (exonym) names; see [`City::alt_names`].
    alt_names: &'static str,
    /// ISO 3166-1 alpha-2 code, e.g. `BR`.
    pub country_code: &'static str,
    pub country_name: &'static str,
    /// First-level administrative division (state, province ...); may be empty.
    pub admin1: &'static str,
    pub lat_deg: f64,
    /// East-positive.
    pub lon_deg: f64,
    /// GeoNames elevation, or the SRTM/GTOPO30 `dem` value when missing, m.
    pub elevation_m: f64,
    pub population: u64,
    /// IANA time-zone identifier, e.g. `America/Sao_Paulo`.
    pub timezone: &'static str,
    /// Standard-time UTC offset, hours (GeoNames `timeZones.txt` raw offset).
    pub std_offset_hours: f64,
    /// Daylight-saving UTC offset, hours; equals `std_offset_hours` where DST
    /// is not observed. Taken from the January/July offsets in
    /// `timeZones.txt` for the year of the data snapshot.
    pub dst_offset_hours: f64,
}

impl City {
    pub fn location(&self) -> GeoLocation {
        GeoLocation::new(self.lat_deg, self.lon_deg, self.elevation_m)
    }

    /// Alternate names kept for searching (Latin-script exonyms of larger
    /// cities: `Mecca` for Makkah, `Bombay` for Mumbai ...).
    pub fn alt_names(&self) -> impl Iterator<Item = &'static str> {
        self.alt_names.split('|').filter(|s| !s.is_empty())
    }

    /// Whether the time zone observes daylight-saving time.
    pub fn observes_dst(&self) -> bool {
        self.dst_offset_hours != self.std_offset_hours
    }

    /// `Name, Admin1, Country`, omitting an empty or repeated admin-1 name.
    pub fn display_name(&self) -> String {
        if self.admin1.is_empty() || self.admin1 == self.name {
            format!("{}, {}", self.name, self.country_name)
        } else {
            format!("{}, {}, {}", self.name, self.admin1, self.country_name)
        }
    }
}

/// A country with at least one city in the atlas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Country {
    /// ISO 3166-1 alpha-2.
    pub code: &'static str,
    /// ISO 3166-1 alpha-3.
    pub iso3: &'static str,
    pub name: &'static str,
    /// Range of this country's cities in [`Atlas::cities`].
    range: (u32, u32),
}

/// Error from parsing an atlas file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "atlas line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

/// Folded search keys of one city.
#[derive(Debug)]
struct Keys {
    /// Folded name (and ASCII name when it folds differently), `|`-separated.
    primary: Box<str>,
    /// Folded alternate names, `|`-separated; may be empty.
    alts: Box<str>,
}

/// How well a city matched a query; lower is better.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchQuality {
    ExactName,
    ExactAltName,
    NamePrefix,
    AltNamePrefix,
    NameWordPrefix,
    AltNameWordPrefix,
    NameSubstring,
}

/// The city atlas.
#[derive(Debug)]
pub struct Atlas {
    /// Sorted by country code, then population (descending).
    cities: Vec<City>,
    keys: Vec<Keys>,
    countries: Vec<Country>,
    /// Folded country name, code and ISO3 per country (same order).
    country_keys: Vec<[String; 3]>,
    /// Folded admin-1 name per city (empty if none).
    admin1_keys: Vec<Box<str>>,
    /// Spatial index: `cell_start[c]..cell_start[c + 1]` indexes `cell_items`.
    cell_start: Vec<u32>,
    cell_items: Vec<u32>,
    source: &'static str,
}

impl Atlas {
    /// The embedded GeoNames atlas, parsed on first use (a few tens of ms) and
    /// shared afterwards.
    pub fn builtin() -> &'static Atlas {
        static ATLAS: OnceLock<Atlas> = OnceLock::new();
        ATLAS.get_or_init(|| Atlas::parse(BUILTIN_TSV).expect("embedded atlas is valid"))
    }

    /// Parses an atlas file in the format written by
    /// `tools/atlas/build_atlas.py` (see `tools/atlas/README.md`).
    pub fn parse(text: &'static str) -> Result<Atlas, ParseError> {
        Parser::new(text).run()
    }

    /// All cities, grouped by country code and sorted by population within
    /// each country.
    pub fn cities(&self) -> &[City] {
        &self.cities
    }

    pub fn len(&self) -> usize {
        self.cities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cities.is_empty()
    }

    /// The `#source` line of the data file (dataset and licence).
    pub fn source(&self) -> &'static str {
        self.source
    }

    /// Countries with at least one city, sorted by ISO code.
    pub fn countries(&self) -> &[Country] {
        &self.countries
    }

    /// Cities of a country (ISO 3166-1 alpha-2, case-insensitive), largest
    /// first. Empty for an unknown code.
    pub fn by_country(&self, code: &str) -> &[City] {
        match self
            .countries
            .binary_search_by(|c| cmp_ascii_ci(c.code, code))
        {
            Ok(i) => {
                let (a, b) = self.countries[i].range;
                &self.cities[a as usize..b as usize]
            }
            Err(_) => &[],
        }
    }

    /// Searches by name, case- and diacritic-insensitively.
    ///
    /// Results are ranked by [`MatchQuality`] (exact name, exact alternate
    /// name, name prefix, alternate-name prefix, word prefix, substring), then
    /// by population. Text after a comma qualifies the place: `Cairo, Egypt`,
    /// `London, Ontario`, `Paris, FR`, `Springfield, Illinois, USA`. Each
    /// qualifier must match the country (name, ISO code or ISO3) or the
    /// admin-1 division.
    pub fn search(&self, query: &str, limit: usize) -> Vec<&City> {
        self.search_ranked(query, limit)
            .into_iter()
            .map(|(c, _)| c)
            .collect()
    }

    /// [`Atlas::search`] with the match quality of each hit.
    pub fn search_ranked(&self, query: &str, limit: usize) -> Vec<(&City, MatchQuality)> {
        let Some(q) = Query::new(query) else {
            return Vec::new();
        };
        let mut hits: Vec<(MatchQuality, u64, u32)> = Vec::new();
        for (i, k) in self.keys.iter().enumerate() {
            let Some(m) = match_keys(&k.primary, &k.alts, &q.name) else {
                continue;
            };
            if !q.qualifiers.is_empty() && !self.qualifies(i, &q.qualifiers) {
                continue;
            }
            hits.push((m, self.cities[i].population, i as u32));
        }
        hits.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
        hits.truncate(limit);
        hits.into_iter()
            .map(|(m, _, i)| (&self.cities[i as usize], m))
            .collect()
    }

    fn qualifies(&self, city: usize, qualifiers: &[String]) -> bool {
        let country = self.country_index(city);
        let [cname, ccode, ciso3] = &self.country_keys[country];
        let admin = &*self.admin1_keys[city];
        qualifiers.iter().all(|q| {
            let alias = match q.as_str() {
                "uk" | "great britain" | "britain" | "england" | "scotland" | "wales" => "gb",
                "uae" => "ae",
                "ksa" => "sa",
                "america" | "united states of america" => "us",
                _ => "",
            };
            q == ccode
                || q == ciso3
                || alias == ccode
                || word_prefix(cname, q)
                || (!admin.is_empty() && word_prefix(admin, q))
        })
    }

    fn country_index(&self, city: usize) -> usize {
        let city = city as u32;
        self.countries.partition_point(|c| c.range.1 <= city)
    }

    /// The city nearest to a point (great-circle distance), or `None` for an
    /// empty atlas.
    pub fn nearest(&self, lat_deg: f64, lon_deg: f64) -> Option<&City> {
        self.nearest_km(lat_deg, lon_deg).map(|(c, _)| c)
    }

    /// The nearest city and its distance in km.
    ///
    /// Uses a 2° bucket grid: a ring search finds a first candidate, then every
    /// cell overlapping that candidate's distance circle is checked, so the
    /// result is exact. Typical cost is a few dozen distance evaluations.
    pub fn nearest_km(&self, lat_deg: f64, lon_deg: f64) -> Option<(&City, f64)> {
        if self.cities.is_empty() || !lat_deg.is_finite() || !lon_deg.is_finite() {
            return None;
        }
        let lat = lat_deg.clamp(-90.0, 90.0);
        let lon = wrap_lon(lon_deg);
        let (r0, c0) = cell_of(lat, lon);

        // 1. Ring search for any candidate.
        let mut best: Option<(u32, f64)> = None;
        let mut ring = 0usize;
        while best.is_none() {
            let rows = r0.saturating_sub(ring)..=(r0 + ring).min(GRID_LAT - 1);
            for r in rows {
                let on_edge_row = r + ring == r0 || r == r0 + ring;
                let span = ring.min(GRID_LON / 2);
                for dc in -(span as isize)..=(span as isize) {
                    if !on_edge_row && dc.unsigned_abs() != ring {
                        continue;
                    }
                    let c = (c0 as isize + dc).rem_euclid(GRID_LON as isize) as usize;
                    self.scan_cell(r, c, lat, lon, &mut best);
                }
            }
            ring += 1;
            if ring > GRID_LAT.max(GRID_LON) {
                break;
            }
        }
        let (_, best_km) = best?;

        // 2. Exhaustive check of the bounding box of the best-distance circle.
        let d = best_km / EARTH_RADIUS_KM; // angular radius, rad
        let d_deg = d.to_degrees();
        let lat_lo = (lat - d_deg).max(-90.0);
        let lat_hi = (lat + d_deg).min(90.0);
        let full_lon = lat_lo <= -90.0 + 1e-9 || lat_hi >= 90.0 - 1e-9 || {
            let s = d.sin() / lat.to_radians().cos();
            s >= 1.0
        };
        let (rlo, _) = cell_of(lat_lo, 0.0);
        let (rhi, _) = cell_of(lat_hi, 0.0);
        if full_lon {
            for r in rlo..=rhi {
                for c in 0..GRID_LON {
                    self.scan_cell(r, c, lat, lon, &mut best);
                }
            }
        } else {
            let dlon = (d.sin() / lat.to_radians().cos()).asin().to_degrees();
            let ncols = ((2.0 * dlon) / CELL_DEG).ceil() as usize + 2;
            let (_, cstart) = cell_of(lat, wrap_lon(lon - dlon));
            for r in rlo..=rhi {
                for k in 0..ncols.min(GRID_LON) {
                    self.scan_cell(r, (cstart + k) % GRID_LON, lat, lon, &mut best);
                }
            }
        }
        let (i, km) = best?;
        Some((&self.cities[i as usize], km))
    }

    fn scan_cell(&self, r: usize, c: usize, lat: f64, lon: f64, best: &mut Option<(u32, f64)>) {
        let cell = r * GRID_LON + c;
        let items =
            &self.cell_items[self.cell_start[cell] as usize..self.cell_start[cell + 1] as usize];
        for &i in items {
            let city = &self.cities[i as usize];
            let km = distance_km(lat, lon, city.lat_deg, city.lon_deg);
            if best.is_none_or(|(_, b)| km < b) {
                *best = Some((i, km));
            }
        }
    }
}

/// Great-circle distance by the haversine formula, km.
pub fn distance_km(lat1_deg: f64, lon1_deg: f64, lat2_deg: f64, lon2_deg: f64) -> f64 {
    let (p1, p2) = (lat1_deg.to_radians(), lat2_deg.to_radians());
    let dp = p2 - p1;
    let dl = (lon2_deg - lon1_deg).to_radians();
    let h = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * h.sqrt().min(1.0).asin()
}

fn wrap_lon(lon: f64) -> f64 {
    let l = (lon + 180.0).rem_euclid(360.0) - 180.0;
    if l >= 180.0 { -180.0 } else { l }
}

fn cell_of(lat: f64, lon: f64) -> (usize, usize) {
    let r = (((lat + 90.0) / CELL_DEG) as usize).min(GRID_LAT - 1);
    let c = (((lon + 180.0) / CELL_DEG) as usize).min(GRID_LON - 1);
    (r, c)
}

fn cmp_ascii_ci(a: &str, b: &str) -> std::cmp::Ordering {
    a.bytes()
        .map(|x| x.to_ascii_uppercase())
        .cmp(b.bytes().map(|x| x.to_ascii_uppercase()))
}

/// A parsed search query.
pub(crate) struct Query {
    pub name: String,
    pub qualifiers: Vec<String>,
}

impl Query {
    pub(crate) fn new(query: &str) -> Option<Query> {
        let mut parts = query.split(',');
        let name = fold(parts.next().unwrap_or(""));
        if name.is_empty() {
            return None;
        }
        let qualifiers = parts.map(fold).filter(|q| !q.is_empty()).collect();
        Some(Query { name, qualifiers })
    }
}

/// Matches a folded query against `|`-separated folded primary and alternate
/// keys.
///
/// The query never contains `|` (folding turns it into a space), so a single
/// `contains` over each joined key string rejects non-matching cities before
/// any per-name work.
pub(crate) fn match_keys(primary: &str, alts: &str, q: &str) -> Option<MatchQuality> {
    use MatchQuality::*;
    let in_prim = primary.contains(q);
    let in_alts = !alts.is_empty() && alts.contains(q);
    if !in_prim && !in_alts {
        return None;
    }
    // (exact, prefix, word prefix, substring) found in each key set.
    let grade = |keys: &str| {
        let mut g = 4u8;
        for s in keys.split('|') {
            let Some(pos) = s.find(q) else { continue };
            let this = if s.len() == q.len() {
                0
            } else if pos == 0 {
                1
            } else if word_prefix(s, q) {
                2
            } else {
                3
            };
            g = g.min(this);
            if g == 0 {
                break;
            }
        }
        g
    };
    let p = if in_prim { grade(primary) } else { 4 };
    let a = if in_alts { grade(alts) } else { 4 };
    match (p, a) {
        (0, _) => Some(ExactName),
        (_, 0) => Some(ExactAltName),
        (1, _) => Some(NamePrefix),
        (_, 1) => Some(AltNamePrefix),
        (2, _) => Some(NameWordPrefix),
        (_, 2) => Some(AltNameWordPrefix),
        (3, _) => Some(NameSubstring),
        _ => None,
    }
}

/// Folded `|`-joined keys from a list of names, dropping duplicates.
fn join_keys<'a>(names: impl Iterator<Item = &'a str>, exclude: &str) -> String {
    let mut out = String::new();
    for n in names {
        let k = fold(n);
        if k.is_empty() || exclude.split('|').any(|e| e == k) || out.split('|').any(|e| e == k) {
            continue;
        }
        if !out.is_empty() {
            out.push('|');
        }
        out.push_str(&k);
    }
    out
}

struct Parser {
    lines: std::iter::Enumerate<std::str::Lines<'static>>,
    source: &'static str,
}

impl Parser {
    fn new(text: &'static str) -> Self {
        Parser {
            lines: text.lines().enumerate(),
            source: "",
        }
    }

    fn err(line: usize, message: impl Into<String>) -> ParseError {
        ParseError {
            line: line + 1,
            message: message.into(),
        }
    }

    /// Next non-comment line, recording `#source`.
    fn next_line(&mut self) -> Option<(usize, &'static str)> {
        for (n, l) in self.lines.by_ref() {
            if let Some(rest) = l.strip_prefix("#source\t") {
                self.source = rest;
            } else if !l.starts_with('#') && !l.is_empty() {
                return Some((n, l));
            }
        }
        None
    }

    fn section(&mut self, name: &str) -> Result<(usize, usize), ParseError> {
        let (n, l) = self.next_line().ok_or_else(|| ParseError {
            line: 0,
            message: format!("missing section @{name}"),
        })?;
        let count = l
            .strip_prefix('@')
            .and_then(|s| s.split_once('\t'))
            .filter(|(s, _)| *s == name)
            .and_then(|(_, c)| c.parse().ok())
            .ok_or_else(|| Self::err(n, format!("expected `@{name}\\t<count>`")))?;
        Ok((n, count))
    }

    fn rows(
        &mut self,
        name: &str,
        fields: usize,
    ) -> Result<Vec<(usize, Vec<&'static str>)>, ParseError> {
        let (n0, count) = self.section(name)?;
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            let (n, l) = self
                .next_line()
                .ok_or_else(|| Self::err(n0, format!("@{name}: fewer than {count} rows")))?;
            let f: Vec<&'static str> = l.split('\t').collect();
            if f.len() != fields {
                return Err(Self::err(
                    n,
                    format!("expected {fields} fields, found {}", f.len()),
                ));
            }
            out.push((n, f));
        }
        Ok(out)
    }

    fn run(mut self) -> Result<Atlas, ParseError> {
        let first = self.lines.clone().next().map(|(_, l)| l).unwrap_or("");
        if first != "#mooncalc-atlas\t1" {
            return Err(Self::err(0, "not a version-1 mooncalc atlas file"));
        }
        let countries_raw = self.rows("countries", 3)?;
        let admin_raw = self.rows("admin1", 2)?;
        let tz_raw = self.rows("timezones", 3)?;

        let mut tzs = Vec::with_capacity(tz_raw.len());
        for (n, f) in &tz_raw {
            tzs.push((f[0], num::<f64>(*n, f[1])?, num::<f64>(*n, f[2])?));
        }
        let mut admins = Vec::with_capacity(admin_raw.len());
        for (n, f) in &admin_raw {
            admins.push((index(*n, f[0], countries_raw.len())?, f[1]));
        }

        let (n0, count) = self.section("cities")?;
        let mut cities = Vec::with_capacity(count);
        let mut keys = Vec::with_capacity(count);
        let mut country_of = Vec::with_capacity(count);
        let mut admin1_keys = Vec::with_capacity(count);
        let admin_folded: Vec<Box<str>> = admins.iter().map(|(_, a)| fold(a).into()).collect();
        for _ in 0..count {
            let (n, l) = self
                .next_line()
                .ok_or_else(|| Self::err(n0, format!("@cities: fewer than {count} rows")))?;
            let f: Vec<&'static str> = l.split('\t').collect();
            if f.len() != 10 {
                return Err(Self::err(
                    n,
                    format!("expected 10 fields, found {}", f.len()),
                ));
            }
            let ci = index(n, f[3], countries_raw.len())?;
            if country_of.last().is_some_and(|&last| last > ci) {
                return Err(Self::err(n, "cities are not grouped by country"));
            }
            let (admin1, admin_key) = if f[4].is_empty() {
                ("", Box::<str>::from(""))
            } else {
                let ai = index(n, f[4], admins.len())?;
                if admins[ai].0 != ci {
                    return Err(Self::err(n, "admin-1 belongs to another country"));
                }
                (admins[ai].1, admin_folded[ai].clone())
            };
            let ti = index(n, f[9], tzs.len())?;
            let lat = num::<f64>(n, f[5])?;
            let lon = num::<f64>(n, f[6])?;
            if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
                return Err(Self::err(n, "coordinates out of range"));
            }
            let name = f[0];
            let ascii_name = if f[1].is_empty() { name } else { f[1] };
            let cc = &countries_raw[ci].1;
            let city = City {
                name,
                ascii_name,
                alt_names: f[2],
                country_code: cc[0],
                country_name: cc[2],
                admin1,
                lat_deg: lat,
                lon_deg: lon,
                elevation_m: num::<i32>(n, f[7])? as f64,
                population: num::<u64>(n, f[8])?,
                timezone: tzs[ti].0,
                std_offset_hours: tzs[ti].1,
                dst_offset_hours: tzs[ti].2,
            };
            let primary = join_keys([name, ascii_name].into_iter(), "");
            let alts = join_keys(city.alt_names(), &primary);
            keys.push(Keys {
                primary: primary.into(),
                alts: alts.into(),
            });
            cities.push(city);
            country_of.push(ci);
            admin1_keys.push(admin_key);
        }

        // Countries actually present, with their city ranges.
        let mut countries = Vec::new();
        let mut country_keys = Vec::new();
        let mut start = 0usize;
        while start < cities.len() {
            let ci = country_of[start];
            let end = start + country_of[start..].iter().take_while(|&&c| c == ci).count();
            let f = &countries_raw[ci].1;
            countries.push(Country {
                code: f[0],
                iso3: f[1],
                name: f[2],
                range: (start as u32, end as u32),
            });
            country_keys.push([
                fold(f[2]),
                f[0].to_ascii_lowercase(),
                f[1].to_ascii_lowercase(),
            ]);
            start = end;
        }

        // Spatial index (counting sort into cells).
        let mut cell_start = vec![0u32; GRID_LAT * GRID_LON + 1];
        let cells: Vec<usize> = cities
            .iter()
            .map(|c| {
                let (r, col) = cell_of(c.lat_deg, wrap_lon(c.lon_deg));
                r * GRID_LON + col
            })
            .collect();
        for &c in &cells {
            cell_start[c + 1] += 1;
        }
        for i in 1..cell_start.len() {
            cell_start[i] += cell_start[i - 1];
        }
        let mut fill = cell_start.clone();
        let mut cell_items = vec![0u32; cities.len()];
        for (i, &c) in cells.iter().enumerate() {
            cell_items[fill[c] as usize] = i as u32;
            fill[c] += 1;
        }

        Ok(Atlas {
            cities,
            keys,
            countries,
            country_keys,
            admin1_keys,
            cell_start,
            cell_items,
            source: self.source,
        })
    }
}

fn num<T: std::str::FromStr>(line: usize, s: &str) -> Result<T, ParseError> {
    s.parse()
        .map_err(|_| Parser::err(line, format!("invalid number `{s}`")))
}

fn index(line: usize, s: &str, len: usize) -> Result<usize, ParseError> {
    let i: usize = num(line, s)?;
    if i < len {
        Ok(i)
    } else {
        Err(Parser::err(line, format!("index {i} out of range")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first(q: &str) -> &'static City {
        Atlas::builtin()
            .search(q, 5)
            .first()
            .copied()
            .unwrap_or_else(|| panic!("no hit for {q:?}"))
    }

    #[test]
    fn builtin_parses_with_expected_size() {
        let a = Atlas::builtin();
        // GeoNames cities15000 has ~34 000 rows (34 214 in the 2026-11-11
        // snapshot); allow for drift on regeneration.
        assert!((30_000..40_000).contains(&a.len()), "{} cities", a.len());
        assert!(a.countries().len() > 230);
        assert!(a.source().contains("GeoNames"));
        // Cities are grouped by country and sorted by population.
        for c in a.countries() {
            let cs = a.by_country(c.code);
            assert!(!cs.is_empty());
            assert!(cs.iter().all(|x| x.country_code == c.code));
            assert!(cs.windows(2).all(|w| w[0].population >= w[1].population));
        }
    }

    #[test]
    fn search_mecca_makkah() {
        let c = first("mecca");
        assert_eq!((c.name, c.country_code), ("Makkah", "SA"));
        assert_eq!(first("Makkah").name, "Makkah");
        assert_eq!(first("MAKKAH").name, "Makkah");
        assert_eq!(c.timezone, "Asia/Riyadh");
        assert_eq!((c.std_offset_hours, c.dst_offset_hours), (3.0, 3.0));
        assert!((c.lat_deg - 21.4266).abs() < 1e-3 && (c.lon_deg - 39.8256).abs() < 1e-3);
    }

    #[test]
    fn search_london() {
        let c = first("london");
        assert_eq!((c.name, c.country_code), ("London", "GB"));
        assert_eq!(c.admin1, "England");
        assert_eq!(c.timezone, "Europe/London");
        assert_eq!((c.std_offset_hours, c.dst_offset_hours), (0.0, 1.0));
        assert_eq!(c.display_name(), "London, England, United Kingdom");
        // Qualified: London, Ontario.
        let c = first("London, Ontario");
        assert_eq!((c.name, c.country_code), ("London", "CA"));
        assert_eq!(first("london, ca").country_code, "CA");
        assert_eq!(first("london, uk").country_code, "GB");
    }

    #[test]
    fn search_new_york() {
        let c = first("new york");
        assert_eq!((c.name, c.country_code), ("New York City", "US"));
        assert_eq!(c.timezone, "America/New_York");
        assert_eq!((c.std_offset_hours, c.dst_offset_hours), (-5.0, -4.0));
        assert_eq!(first("new york, usa").name, "New York City");
        // Word prefix.
        assert_eq!(first("york city").name, "New York City");
    }

    #[test]
    fn search_sao_paulo_with_and_without_accent() {
        for q in [
            "são paulo",
            "Sao Paulo",
            "SAO PAULO",
            "sao paulo, brazil",
            "são",
        ] {
            let c = first(q);
            assert_eq!((c.name, c.country_code), ("São Paulo", "BR"), "{q}");
        }
        assert_eq!(first("são paulo").ascii_name, "Sao Paulo");
    }

    #[test]
    fn search_city_country() {
        let c = first("Cairo, Egypt");
        assert_eq!((c.name, c.country_code), ("Cairo", "EG"));
        assert_eq!(first("cairo, eg").country_code, "EG");
        assert_eq!(first("cairo, egy").country_code, "EG");
        assert!(Atlas::builtin().search("Cairo, Atlantis", 5).is_empty());
    }

    #[test]
    fn search_ranking() {
        let a = Atlas::builtin();
        // A primary-name match outranks a city that merely has the query as
        // an alternate name (Jerusalem lists "Salem").
        let hits = a.search_ranked("salem", 3);
        assert_eq!(hits[0].1, MatchQuality::ExactName);
        assert_ne!(hits[0].0.name, "Jerusalem");
        // Prefix results are ranked by population.
        let hits = a.search("lon", 10);
        assert_eq!(hits[0].name, "London");
        assert!(hits.len() == 10);
        assert!(a.search("", 10).is_empty());
        assert!(a.search(" , ", 10).is_empty());
        assert!(a.search("zzzzqqq", 10).is_empty());
        // Exonyms.
        assert_eq!(first("bombay").name, "Mumbai");
        assert_eq!(first("peking").name, "Beijing");
        assert_eq!(first("Hanoi").country_code, "VN");
        assert_eq!(first("hà nội").country_code, "VN");
    }

    #[test]
    fn nearest_city() {
        let a = Atlas::builtin();
        let near = |lat, lon| a.nearest(lat, lon).unwrap().name;
        assert_eq!(near(21.4225, 39.8262), "Makkah"); // the Kaaba
        assert_eq!(near(51.508, -0.128), "London"); // Trafalgar Square
        assert_eq!(near(-23.5475, -46.6361), "São Paulo");
        assert_eq!(near(30.0626, 31.2497), "Cairo");
        assert_eq!(near(40.7143, -74.006), "New York City");
        // Across the antimeridian: Suva, Fiji (178.44 E) from 179.99 W.
        let (c, km) = a.nearest_km(-18.1, -179.99).unwrap();
        assert_eq!(c.country_code, "FJ");
        assert!(km < 400.0, "{km}");
        // Far from anything, near the poles: still a result.
        assert!(a.nearest(-89.9, 0.0).is_some());
        assert!(a.nearest(89.9, 0.0).is_some());
        assert!(a.nearest(f64::NAN, 0.0).is_none());
    }

    #[test]
    fn nearest_matches_brute_force() {
        let a = Atlas::builtin();
        let mut s: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut rnd = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            (s >> 11) as f64 / (1u64 << 53) as f64
        };
        for _ in 0..200 {
            let lat = rnd() * 180.0 - 90.0;
            let lon = rnd() * 360.0 - 180.0;
            let (got, km) = a.nearest_km(lat, lon).unwrap();
            let best = a
                .cities()
                .iter()
                .map(|c| distance_km(lat, lon, c.lat_deg, c.lon_deg))
                .fold(f64::INFINITY, f64::min);
            assert!(
                (km - best).abs() < 1e-9,
                "{lat},{lon}: {} {km} vs {best}",
                got.name
            );
        }
    }

    #[test]
    fn by_country_lookup() {
        let a = Atlas::builtin();
        let sa = a.by_country("sa");
        assert_eq!(sa[0].country_name, "Saudi Arabia");
        assert!(sa.iter().any(|c| c.name == "Makkah"));
        assert!(a.by_country("XX").is_empty());
        assert!(a.by_country("").is_empty());
    }

    #[test]
    fn location_uses_elevation() {
        let c = first("são paulo");
        let loc = c.location();
        assert_eq!(loc, GeoLocation::new(c.lat_deg, c.lon_deg, 769.0));
    }

    #[test]
    fn distance_reference() {
        // London–Paris is about 344 km.
        let d = distance_km(51.5074, -0.1278, 48.8566, 2.3522);
        assert!((d - 343.5).abs() < 2.0, "{d}");
    }

    #[test]
    fn rejects_bad_files() {
        assert!(Atlas::parse("hello").is_err());
        let ok = "#mooncalc-atlas\t1\n@countries\t1\nXX\tXXX\tTestland\n@admin1\t0\n@timezones\t1\nEtc/UTC\t0\t0\n@cities\t1\nA\t\t\t0\t\t1\t2\t3\t4\t0\n";
        let a = Atlas::parse(ok).unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(a.nearest(0.0, 0.0).unwrap().name, "A");
        let bad = "#mooncalc-atlas\t1\n@countries\t1\nXX\tXXX\tTestland\n@admin1\t0\n@timezones\t1\nEtc/UTC\t0\t0\n@cities\t1\nA\t\t\t0\t\t95\t2\t3\t4\t0\n";
        assert!(Atlas::parse(bad).is_err());
        let short = "#mooncalc-atlas\t1\n@countries\t1\nXX\tXXX\tTestland\n@admin1\t0\n@timezones\t1\nEtc/UTC\t0\t0\n@cities\t2\nA\t\t\t0\t\t1\t2\t3\t4\t0\n";
        assert!(Atlas::parse(short).is_err());
    }
}
