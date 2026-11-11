# Embedded base map

Coastlines, land polygons and country borders embedded in `mooncalc-maps` (see `crates/mooncalc-maps/src/basemap.rs`).

## Source and licence

[Natural Earth](https://www.naturalearthdata.com/) vector data, version 5.1.2, taken from the GeoJSON in the [`nvkelso/natural-earth-vector`](https://github.com/nvkelso/natural-earth-vector) repository at tag `v5.1.2`. Natural Earth is in the **public domain** ([terms of use](https://www.naturalearthdata.com/about/terms-of-use/)); no attribution is required, but "Made with Natural Earth" is appreciated.

| File | Natural Earth layer | Items | Points | Bytes |
| --- | --- | ---: | ---: | ---: |
| `coastline_110m.mcbm` | `ne_110m_coastline` | 134 polylines | 5,128 | 20,188 |
| `land_110m.mcbm` | `ne_110m_land` | 127 polygons | 5,004 | 19,807 |
| `borders_110m.mcbm` | `ne_110m_admin_0_boundary_lines_land` | 333 polylines | 2,995 | 12,358 |
| `coastline_50m.mcbm` | `ne_50m_coastline` | 1,429 polylines | 60,416 | 177,281 |
| `land_50m.mcbm` | `ne_50m_land` | 1,421 polygons | 59,247 | 175,732 |
| `borders_50m.mcbm` | `ne_50m_admin_0_boundary_lines_land` | 393 polylines | 19,758 | 56,091 |

Total: about 450 KiB.

## Rebuilding

```sh
python -I tools/basemap/build_basemap.py --cache <download dir> --out assets/basemap
```

The script downloads the six GeoJSON files into the cache directory (with a generic User-Agent), checks their SHA-256 against the values pinned in the script, and writes the `.mcbm` files. The output is byte-for-byte reproducible. It needs only the Python standard library.

## `MCBM` format, version 1

All integers are little-endian.

| Offset | Size | Field |
| ---: | ---: | --- |
| 0 | 4 | Magic `MCBM` |
| 4 | 1 | Version, `1` |
| 5 | 1 | Geometry kind: `1` polylines, `2` polygons |
| 6 | 2 | Reserved, `0` |
| 8 | 4 | `u32` quantisation units per degree (currently 1000, about 111 m) |
| 12 | 4 | `u32` item count |
| 16 | 4 | `u32` total point count |
| 20 | … | Body |

- **Polyline item:** a varint point count (at least 2), then the points.
- **Polygon item:** a varint ring count (at least 1; ring 0 is the outer ring, the rest are holes), then for each ring a varint point count (at least 3) and the points. Rings are implicitly closed: the closing copy of the first point is not stored. Natural Earth multipolygons are split into separate polygons.
- **Points:** `(lat, lon)` pairs. Each coordinate is the difference from the previous point of the same polyline or ring, in quantised units, zigzag-encoded (`(n << 1) ^ (n >> 63)`) and written as an unsigned LEB128 varint. The first point of each polyline or ring is relative to `(0, 0)`. Consecutive duplicate points after quantisation are dropped.

Longitudes stay within [-180, 180]: Natural Earth already splits geometry at the antimeridian.
