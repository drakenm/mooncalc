//! Embedded base-map geometry: coastlines, land polygons and country
//! borders from [Natural Earth](https://www.naturalearthdata.com/) (public
//! domain), at the 1:110m and 1:50m scales.
//!
//! The data is converted offline by `tools/basemap/build_basemap.py` into the
//! compact `MCBM` format (quantised, delta- and varint-encoded lat/lon; see
//! `assets/basemap/README.md`) and embedded with `include_bytes!`, so nothing
//! is read from disk at run time.
//!
//! * [`LayerData`] parses one `MCBM` blob and iterates over its polylines or
//!   polygons without allocating the whole layer.
//! * [`BaseMap::embedded`] returns the fully decoded layers for a [`Scale`],
//!   decoded once and cached for the life of the process, plus a lazily
//!   built [`LandMask`] used for land/sea shading in any projection.

use std::fmt;
use std::sync::OnceLock;

/// Natural Earth scale of the embedded data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Scale {
    /// 1:110m: small, coarse; good for thumbnails and globes.
    #[default]
    Ne110m,
    /// 1:50m: detailed enough for full-screen world maps.
    Ne50m,
}

/// A point on the globe, degrees (latitude north-positive, longitude
/// east-positive in [-180, 180]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeoPoint {
    pub lat_deg: f32,
    pub lon_deg: f32,
}

/// A polygon: `rings[0]` is the outer boundary, any further rings are holes.
/// Rings are implicitly closed (the last point connects back to the first).
#[derive(Clone, Debug, PartialEq)]
pub struct Polygon {
    pub rings: Vec<Vec<GeoPoint>>,
}

/// Geometry kind stored in an `MCBM` layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeometryKind {
    Polylines,
    Polygons,
}

/// Why an `MCBM` blob could not be parsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    TooShort,
    BadMagic,
    UnsupportedVersion(u8),
    UnknownKind(u8),
    ZeroUnits,
    /// The body ended in the middle of an item, or a varint overflowed.
    Truncated,
    /// An item had too few points/rings.
    Degenerate,
    /// Item or point counts disagree with the header, or bytes remain.
    CountMismatch,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort => f.write_str("base-map blob shorter than its header"),
            Self::BadMagic => f.write_str("not an MCBM base-map blob"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported MCBM version {v}"),
            Self::UnknownKind(k) => write!(f, "unknown MCBM geometry kind {k}"),
            Self::ZeroUnits => f.write_str("MCBM quantisation of zero units per degree"),
            Self::Truncated => f.write_str("MCBM body truncated or malformed varint"),
            Self::Degenerate => f.write_str("MCBM item with too few points or rings"),
            Self::CountMismatch => f.write_str("MCBM counts disagree with the header"),
        }
    }
}

impl std::error::Error for ParseError {}

const HEADER_LEN: usize = 20;

/// A validated `MCBM` layer, borrowing its bytes.
///
/// [`LayerData::parse`] walks the whole body once, so the iterators returned
/// by [`polylines`](Self::polylines) and [`polygons`](Self::polygons) are
/// infallible.
#[derive(Clone, Copy, Debug)]
pub struct LayerData<'a> {
    kind: GeometryKind,
    units_per_degree: u32,
    items: u32,
    points: u32,
    body: &'a [u8],
}

impl<'a> LayerData<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ParseError> {
        if bytes.len() < HEADER_LEN {
            return Err(ParseError::TooShort);
        }
        if &bytes[0..4] != b"MCBM" {
            return Err(ParseError::BadMagic);
        }
        if bytes[4] != 1 {
            return Err(ParseError::UnsupportedVersion(bytes[4]));
        }
        let kind = match bytes[5] {
            1 => GeometryKind::Polylines,
            2 => GeometryKind::Polygons,
            k => return Err(ParseError::UnknownKind(k)),
        };
        let u32_at =
            |o: usize| u32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
        let units_per_degree = u32_at(8);
        if units_per_degree == 0 {
            return Err(ParseError::ZeroUnits);
        }
        let layer = Self {
            kind,
            units_per_degree,
            items: u32_at(12),
            points: u32_at(16),
            body: &bytes[HEADER_LEN..],
        };
        layer.validate()?;
        Ok(layer)
    }

    fn validate(&self) -> Result<(), ParseError> {
        let mut r = Reader {
            buf: self.body,
            pos: 0,
        };
        let mut points = 0u64;
        for _ in 0..self.items {
            let rings = match self.kind {
                GeometryKind::Polylines => 1,
                GeometryKind::Polygons => {
                    let n = r.varint()?;
                    if n == 0 {
                        return Err(ParseError::Degenerate);
                    }
                    n
                }
            };
            let min_pts = match self.kind {
                GeometryKind::Polylines => 2,
                GeometryKind::Polygons => 3,
            };
            for _ in 0..rings {
                let n = r.varint()?;
                if n < min_pts {
                    return Err(ParseError::Degenerate);
                }
                for _ in 0..n * 2 {
                    r.varint()?;
                }
                points += n;
            }
        }
        if points != u64::from(self.points) || r.pos != self.body.len() {
            return Err(ParseError::CountMismatch);
        }
        Ok(())
    }

    pub fn kind(&self) -> GeometryKind {
        self.kind
    }

    /// Number of polylines or polygons.
    pub fn len(&self) -> usize {
        self.items as usize
    }

    pub fn is_empty(&self) -> bool {
        self.items == 0
    }

    /// Total number of points over every item.
    pub fn point_count(&self) -> usize {
        self.points as usize
    }

    /// Iterates over the polylines. Empty if this is a polygon layer.
    pub fn polylines(&self) -> PolylineIter<'a> {
        PolylineIter {
            reader: Reader {
                buf: self.body,
                pos: 0,
            },
            remaining: if self.kind == GeometryKind::Polylines {
                self.items
            } else {
                0
            },
            units: self.units_per_degree as f32,
        }
    }

    /// Iterates over the polygons. Empty if this is a polyline layer.
    pub fn polygons(&self) -> PolygonIter<'a> {
        PolygonIter {
            reader: Reader {
                buf: self.body,
                pos: 0,
            },
            remaining: if self.kind == GeometryKind::Polygons {
                self.items
            } else {
                0
            },
            units: self.units_per_degree as f32,
        }
    }
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn varint(&mut self) -> Result<u64, ParseError> {
        let mut v = 0u64;
        let mut shift = 0;
        loop {
            let b = *self.buf.get(self.pos).ok_or(ParseError::Truncated)?;
            self.pos += 1;
            if shift >= 63 && b > 1 {
                return Err(ParseError::Truncated);
            }
            v |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return Ok(v);
            }
            shift += 7;
        }
    }

    fn zigzag(&mut self) -> Result<i64, ParseError> {
        let v = self.varint()?;
        Ok(((v >> 1) as i64) ^ -((v & 1) as i64))
    }

    /// One delta-encoded point run. Only called on validated data.
    fn points(&mut self, units: f32) -> Vec<GeoPoint> {
        let n = self.varint().unwrap_or(0) as usize;
        let mut out = Vec::with_capacity(n);
        let (mut lat, mut lon) = (0i64, 0i64);
        for _ in 0..n {
            lat += self.zigzag().unwrap_or(0);
            lon += self.zigzag().unwrap_or(0);
            out.push(GeoPoint {
                lat_deg: lat as f32 / units,
                lon_deg: lon as f32 / units,
            });
        }
        out
    }
}

/// Iterator over the polylines of a [`LayerData`].
pub struct PolylineIter<'a> {
    reader: Reader<'a>,
    remaining: u32,
    units: f32,
}

impl Iterator for PolylineIter<'_> {
    type Item = Vec<GeoPoint>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        Some(self.reader.points(self.units))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining as usize, Some(self.remaining as usize))
    }
}

impl ExactSizeIterator for PolylineIter<'_> {}

/// Iterator over the polygons of a [`LayerData`].
pub struct PolygonIter<'a> {
    reader: Reader<'a>,
    remaining: u32,
    units: f32,
}

impl Iterator for PolygonIter<'_> {
    type Item = Polygon;
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        let n = self.reader.varint().unwrap_or(0) as usize;
        let rings = (0..n).map(|_| self.reader.points(self.units)).collect();
        Some(Polygon { rings })
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining as usize, Some(self.remaining as usize))
    }
}

impl ExactSizeIterator for PolygonIter<'_> {}

/// The raw embedded `MCBM` blobs.
pub mod raw {
    pub const COASTLINE_110M: &[u8] = include_bytes!("../../../assets/basemap/coastline_110m.mcbm");
    pub const LAND_110M: &[u8] = include_bytes!("../../../assets/basemap/land_110m.mcbm");
    pub const BORDERS_110M: &[u8] = include_bytes!("../../../assets/basemap/borders_110m.mcbm");
    pub const COASTLINE_50M: &[u8] = include_bytes!("../../../assets/basemap/coastline_50m.mcbm");
    pub const LAND_50M: &[u8] = include_bytes!("../../../assets/basemap/land_50m.mcbm");
    pub const BORDERS_50M: &[u8] = include_bytes!("../../../assets/basemap/borders_50m.mcbm");
}

/// Decoded base-map layers for one [`Scale`].
#[derive(Debug)]
pub struct BaseMap {
    pub scale: Scale,
    /// Coastlines (Natural Earth `coastline`).
    pub coastlines: Vec<Vec<GeoPoint>>,
    /// Land polygons (Natural Earth `land`).
    pub land: Vec<Polygon>,
    /// Land boundaries between countries (Natural Earth
    /// `admin_0_boundary_lines_land`).
    pub borders: Vec<Vec<GeoPoint>>,
    mask: OnceLock<LandMask>,
}

impl BaseMap {
    /// Decodes a base map from `MCBM` blobs.
    pub fn from_blobs(
        scale: Scale,
        coastline: &[u8],
        land: &[u8],
        borders: &[u8],
    ) -> Result<Self, ParseError> {
        let c = LayerData::parse(coastline)?;
        let l = LayerData::parse(land)?;
        let b = LayerData::parse(borders)?;
        Ok(Self {
            scale,
            coastlines: c.polylines().collect(),
            land: l.polygons().collect(),
            borders: b.polylines().collect(),
            mask: OnceLock::new(),
        })
    }

    /// The embedded base map at `scale`, decoded on first use and cached.
    pub fn embedded(scale: Scale) -> &'static BaseMap {
        static S110: OnceLock<BaseMap> = OnceLock::new();
        static S50: OnceLock<BaseMap> = OnceLock::new();
        let (cell, blobs) = match scale {
            Scale::Ne110m => (
                &S110,
                (raw::COASTLINE_110M, raw::LAND_110M, raw::BORDERS_110M),
            ),
            Scale::Ne50m => (&S50, (raw::COASTLINE_50M, raw::LAND_50M, raw::BORDERS_50M)),
        };
        cell.get_or_init(|| {
            Self::from_blobs(scale, blobs.0, blobs.1, blobs.2).expect("embedded base map is valid")
        })
    }

    /// A land/sea raster of this map's land polygons, built on first use.
    pub fn land_mask(&self) -> &LandMask {
        self.mask.get_or_init(|| {
            let (w, h) = match self.scale {
                Scale::Ne110m => (2048, 1024),
                Scale::Ne50m => (4096, 2048),
            };
            LandMask::rasterize(&self.land, w, h)
        })
    }
}

/// A one-bit land/sea raster on an equirectangular lat/lon grid covering
/// the whole globe (column 0 starts at 180° W, row 0 at 90° N).
///
/// Shading land by sampling this mask through a projection's inverse works
/// for every projection without clipping polygons against horizons or
/// seams.
#[derive(Clone, Debug)]
pub struct LandMask {
    width: usize,
    height: usize,
    words_per_row: usize,
    bits: Vec<u64>,
}

impl LandMask {
    /// Scanline-fills `polygons` (even-odd rule over all rings) into a
    /// `width` x `height` mask, sampling at cell centres.
    pub fn rasterize(polygons: &[Polygon], width: usize, height: usize) -> Self {
        let words_per_row = width.div_ceil(64);
        let mut bits = vec![0u64; words_per_row * height];
        // Edges in mask pixel space: (y0, y1, x at y0, dx/dy), y0 < y1.
        let sx = width as f64 / 360.0;
        let sy = height as f64 / 180.0;
        let mut edges: Vec<(f64, f64, f64, f64)> = Vec::new();
        for poly in polygons {
            for ring in &poly.rings {
                for (i, a) in ring.iter().enumerate() {
                    let b = ring[(i + 1) % ring.len()];
                    let (ax, ay) = (
                        (f64::from(a.lon_deg) + 180.0) * sx,
                        (90.0 - f64::from(a.lat_deg)) * sy,
                    );
                    let (bx, by) = (
                        (f64::from(b.lon_deg) + 180.0) * sx,
                        (90.0 - f64::from(b.lat_deg)) * sy,
                    );
                    if ay == by {
                        continue;
                    }
                    let (x0, y0, x1, y1) = if ay < by {
                        (ax, ay, bx, by)
                    } else {
                        (bx, by, ax, ay)
                    };
                    edges.push((y0, y1, x0, (x1 - x0) / (y1 - y0)));
                }
            }
        }
        // Bucket edges by the first row whose centre they cover.
        let mut buckets: Vec<Vec<usize>> = vec![Vec::new(); height];
        for (i, e) in edges.iter().enumerate() {
            let r = (e.0 - 0.5).ceil().max(0.0) as usize;
            if r < height {
                buckets[r].push(i);
            }
        }
        let mut active: Vec<usize> = Vec::new();
        let mut xs: Vec<f64> = Vec::new();
        for (row, bucket) in buckets.iter().enumerate() {
            let yc = row as f64 + 0.5;
            active.extend_from_slice(bucket);
            active.retain(|&i| edges[i].1 > yc);
            xs.clear();
            xs.extend(active.iter().filter(|&&i| edges[i].0 <= yc).map(|&i| {
                let e = edges[i];
                e.2 + (yc - e.0) * e.3
            }));
            xs.sort_by(f64::total_cmp);
            let row_bits = &mut bits[row * words_per_row..(row + 1) * words_per_row];
            for pair in xs.as_chunks::<2>().0 {
                // Pixels whose centre lies inside [x0, x1).
                let c0 = (pair[0] - 0.5).ceil().max(0.0) as usize;
                let c1 = ((pair[1] - 0.5).ceil().max(0.0) as usize).min(width);
                for c in c0..c1 {
                    row_bits[c / 64] |= 1 << (c % 64);
                }
            }
        }
        Self {
            width,
            height,
            words_per_row,
            bits,
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// Whether mask cell (`col`, `row`) is land. `col` wraps around the
    /// globe; `row` is clamped.
    #[inline]
    pub fn cell(&self, col: isize, row: isize) -> bool {
        let c = col.rem_euclid(self.width as isize) as usize;
        let r = row.clamp(0, self.height as isize - 1) as usize;
        self.bits[r * self.words_per_row + c / 64] >> (c % 64) & 1 == 1
    }

    /// Whether the point is on land (nearest mask cell).
    pub fn is_land(&self, lat_deg: f64, lon_deg: f64) -> bool {
        let u = (lon_deg + 180.0) / 360.0 * self.width as f64;
        let v = (90.0 - lat_deg) / 180.0 * self.height as f64;
        self.cell(u.floor() as isize, v.floor() as isize)
    }

    /// Land fraction in [0, 1], bilinearly interpolated between mask cells,
    /// for smooth coast edges.
    #[inline]
    pub fn land_fraction(&self, lat_deg: f64, lon_deg: f64) -> f32 {
        let u = (lon_deg + 180.0) / 360.0 * self.width as f64 - 0.5;
        let v = (90.0 - lat_deg) / 180.0 * self.height as f64 - 0.5;
        let (u0, v0) = (u.floor(), v.floor());
        let (tx, ty) = ((u - u0) as f32, (v - v0) as f32);
        let (c, r) = (u0 as isize, v0 as isize);
        let f = |b: bool| if b { 1.0f32 } else { 0.0 };
        let top = f(self.cell(c, r)) * (1.0 - tx) + f(self.cell(c + 1, r)) * tx;
        let bot = f(self.cell(c, r + 1)) * (1.0 - tx) + f(self.cell(c + 1, r + 1)) * tx;
        top * (1.0 - ty) + bot * ty
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(lines: &[Vec<GeoPoint>]) -> (f32, f32, f32, f32) {
        let mut b = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for p in lines.iter().flatten() {
            b.0 = b.0.min(p.lat_deg);
            b.1 = b.1.max(p.lat_deg);
            b.2 = b.2.min(p.lon_deg);
            b.3 = b.3.max(p.lon_deg);
        }
        b
    }

    #[test]
    fn embedded_layers_parse_with_expected_counts() {
        for (blob, kind, items, points) in [
            (raw::COASTLINE_110M, GeometryKind::Polylines, 134, 5128),
            (raw::LAND_110M, GeometryKind::Polygons, 127, 5004),
            (raw::BORDERS_110M, GeometryKind::Polylines, 333, 2995),
            (raw::COASTLINE_50M, GeometryKind::Polylines, 1429, 60416),
            (raw::LAND_50M, GeometryKind::Polygons, 1421, 59247),
            (raw::BORDERS_50M, GeometryKind::Polylines, 393, 19758),
        ] {
            let layer = LayerData::parse(blob).unwrap();
            assert_eq!(layer.kind(), kind);
            assert_eq!(layer.len(), items);
            assert_eq!(layer.point_count(), points);
            let n: usize = match kind {
                GeometryKind::Polylines => layer.polylines().map(|l| l.len()).sum(),
                GeometryKind::Polygons => layer
                    .polygons()
                    .flat_map(|p| p.rings)
                    .map(|r| r.len())
                    .sum(),
            };
            assert_eq!(n, points);
        }
    }

    #[test]
    fn embedded_bounds_are_global() {
        for scale in [Scale::Ne110m, Scale::Ne50m] {
            let m = BaseMap::embedded(scale);
            let (lat0, lat1, lon0, lon1) = bounds(&m.coastlines);
            assert!((-90.0..-77.0).contains(&lat0), "{lat0}");
            assert!(lat1 > 83.0 && lat1 <= 90.0, "{lat1}");
            assert!((-180.0..-179.0).contains(&lon0), "{lon0}");
            assert!(lon1 <= 180.0 && lon1 > 179.0, "{lon1}");
            let (blat0, blat1, _, _) = bounds(&m.borders);
            assert!(blat0 > -60.0 && blat1 < 75.0);
            assert!(m.land.iter().all(|p| p.rings.iter().all(|r| r.len() >= 3)));
        }
    }

    #[test]
    fn land_mask_knows_land_from_sea() {
        for scale in [Scale::Ne110m, Scale::Ne50m] {
            let mask = BaseMap::embedded(scale).land_mask();
            // Land: central Sahara, Brazil, Siberia, central Australia, Antarctica.
            for (lat, lon) in [
                (23.0, 10.0),
                (-10.0, -55.0),
                (62.0, 100.0),
                (-25.0, 134.0),
                (-85.0, 0.0),
            ] {
                assert!(
                    mask.is_land(lat, lon),
                    "{scale:?} ({lat}, {lon}) should be land"
                );
                assert!(mask.land_fraction(lat, lon) > 0.99);
            }
            // Sea: mid-Pacific, mid-Atlantic, Indian Ocean, Arctic near pole.
            for (lat, lon) in [(0.0, -150.0), (30.0, -40.0), (-30.0, 80.0), (88.0, 0.0)] {
                assert!(
                    !mask.is_land(lat, lon),
                    "{scale:?} ({lat}, {lon}) should be sea"
                );
            }
        }
    }

    #[test]
    fn rejects_bad_blobs() {
        assert_eq!(LayerData::parse(b"MCBM").unwrap_err(), ParseError::TooShort);
        let mut bad = raw::LAND_110M.to_vec();
        bad[0] = b'X';
        assert_eq!(LayerData::parse(&bad).unwrap_err(), ParseError::BadMagic);
        let truncated = &raw::LAND_110M[..raw::LAND_110M.len() - 3];
        assert!(LayerData::parse(truncated).is_err());
        let mut extra = raw::BORDERS_110M.to_vec();
        extra.push(0);
        assert_eq!(
            LayerData::parse(&extra).unwrap_err(),
            ParseError::CountMismatch
        );
    }
}
