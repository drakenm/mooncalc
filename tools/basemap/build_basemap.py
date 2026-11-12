#!/usr/bin/env python3
"""Convert Natural Earth GeoJSON into MoonCalc's compact base-map format.

Reproducible build of the files under ``assets/basemap/``. Usage::

    python -I tools/basemap/build_basemap.py --cache <download dir> --out assets/basemap

The GeoJSON sources are fetched from a pinned tag of the
``nvkelso/natural-earth-vector`` repository (Natural Earth, public domain)
into ``--cache`` unless already present, and each download is checked
against the SHA-256 recorded below. Pass ``--offline`` to refuse network
access. Only the Python standard library is used.

Output format (``MCBM`` version 1, all integers little-endian):

    offset  size  field
    0       4     magic b"MCBM"
    4       1     version (1)
    5       1     geometry kind: 1 = polylines, 2 = polygons
    6       2     reserved (0)
    8       4     u32 quantisation units per degree
    12      4     u32 item count
    16      4     u32 total point count
    20      ...   body

Polyline item: varint point count (>= 2), then the points.
Polygon item:  varint ring count (>= 1; ring 0 is the outer ring, the rest
               are holes), then per ring a varint point count (>= 3) and the
               points. Rings are implicitly closed (the closing duplicate of
               the first point is not stored).

Points are written as (lat, lon) pairs of zigzag-encoded LEB128 varints
holding the delta from the previous point of the same polyline or ring, in
quantised units; the first point of each polyline/ring is relative to
(0, 0). Consecutive duplicate points (after quantisation) are dropped.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import struct
import sys
import urllib.request

NE_TAG = "v5.1.2"
NE_URL = (
    "https://raw.githubusercontent.com/nvkelso/natural-earth-vector/"
    f"{NE_TAG}/geojson/{{name}}.geojson"
)
USER_AGENT = "basemap-builder/1.0"
UNITS_PER_DEGREE = 1000

# (source name, output file, geometry kind, sha256 of the source GeoJSON)
SOURCES = [
    ("ne_110m_coastline", "coastline_110m.mcbm", "lines",
     "851f581ff5ffb844deed8ae1a9ce22e3c4bb3d74fa342cadb5d8e39b41ae7c3c"),
    ("ne_110m_land", "land_110m.mcbm", "polygons",
     "9e0729ee253ca7d7a5c4ae9395fb1902264c5377c52e224d13dd85010e2835d9"),
    ("ne_110m_admin_0_boundary_lines_land", "borders_110m.mcbm", "lines",
     "d42479fd79552cca4eec7f85fcdca717a790d29ff06be7676f1af0568c6d3f7c"),
    ("ne_50m_coastline", "coastline_50m.mcbm", "lines",
     "271f1c4c1908312bac6b29d158ea1356544beafc129f260005300913aa5ea283"),
    ("ne_50m_land", "land_50m.mcbm", "polygons",
     "e874b27a51d146452be360cafb3cc50c86001074a67d534113e6534682f9826b"),
    ("ne_50m_admin_0_boundary_lines_land", "borders_50m.mcbm", "lines",
     "2faac4f6b34386f3d21b6e018cf151f241f00e5c936d44dd17d7d9bfb147fa48"),
]

KIND_CODE = {"lines": 1, "polygons": 2}


def fetch(name: str, sha256: str, cache: pathlib.Path, offline: bool) -> bytes:
    path = cache / f"{name}.geojson"
    if not path.exists():
        if offline:
            sys.exit(f"missing {path} and --offline given")
        url = NE_URL.format(name=name)
        print(f"downloading {url}", file=sys.stderr)
        req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
        with urllib.request.urlopen(req, timeout=60) as resp:
            data = resp.read()
        path.write_bytes(data)
    data = path.read_bytes()
    digest = hashlib.sha256(data).hexdigest()
    if digest != sha256:
        sys.exit(f"{path}: sha256 {digest} does not match expected {sha256}")
    return data


def varint(out: bytearray, v: int) -> None:
    assert v >= 0
    while True:
        b = v & 0x7F
        v >>= 7
        if v:
            out.append(b | 0x80)
        else:
            out.append(b)
            return


def zigzag(v: int) -> int:
    return (v << 1) if v >= 0 else ((-v << 1) - 1)


def quantise(coords) -> list[tuple[int, int]]:
    """GeoJSON [lon, lat] positions -> deduplicated quantised (lat, lon)."""
    pts: list[tuple[int, int]] = []
    for c in coords:
        lon, lat = float(c[0]), float(c[1])
        q = (round(lat * UNITS_PER_DEGREE), round(lon * UNITS_PER_DEGREE))
        if not pts or pts[-1] != q:
            pts.append(q)
    return pts


def write_points(out: bytearray, pts: list[tuple[int, int]]) -> None:
    varint(out, len(pts))
    plat, plon = 0, 0
    for lat, lon in pts:
        varint(out, zigzag(lat - plat))
        varint(out, zigzag(lon - plon))
        plat, plon = lat, lon


def lines_of(geom) -> list:
    t = geom["type"]
    if t == "LineString":
        return [geom["coordinates"]]
    if t == "MultiLineString":
        return list(geom["coordinates"])
    raise ValueError(f"unexpected geometry {t} in a line layer")


def polygons_of(geom) -> list:
    t = geom["type"]
    if t == "Polygon":
        return [geom["coordinates"]]
    if t == "MultiPolygon":
        return list(geom["coordinates"])
    raise ValueError(f"unexpected geometry {t} in a polygon layer")


def convert(data: bytes, kind: str) -> tuple[bytes, int, int]:
    fc = json.loads(data)
    body = bytearray()
    items = 0
    points = 0
    for feature in fc["features"]:
        geom = feature["geometry"]
        if geom is None:
            continue
        if kind == "lines":
            for line in lines_of(geom):
                pts = quantise(line)
                if len(pts) < 2:
                    continue
                write_points(body, pts)
                items += 1
                points += len(pts)
        else:
            for poly in polygons_of(geom):
                rings = []
                for ring in poly:
                    pts = quantise(ring)
                    if len(pts) > 1 and pts[0] == pts[-1]:
                        pts.pop()
                    if len(pts) >= 3:
                        rings.append(pts)
                    elif not rings:
                        break  # degenerate outer ring: drop the polygon
                if not rings:
                    continue
                varint(body, len(rings))
                for pts in rings:
                    write_points(body, pts)
                    points += len(pts)
                items += 1
    header = b"MCBM" + struct.pack(
        "<BBHIII", 1, KIND_CODE[kind], 0, UNITS_PER_DEGREE, items, points
    )
    return header + bytes(body), items, points


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--cache", required=True, type=pathlib.Path,
                    help="directory holding (or receiving) the GeoJSON downloads")
    ap.add_argument("--out", required=True, type=pathlib.Path,
                    help="output directory, normally assets/basemap")
    ap.add_argument("--offline", action="store_true",
                    help="fail instead of downloading missing sources")
    args = ap.parse_args()
    args.cache.mkdir(parents=True, exist_ok=True)
    args.out.mkdir(parents=True, exist_ok=True)
    for name, out_name, kind, sha in SOURCES:
        blob, items, points = convert(fetch(name, sha, args.cache, args.offline), kind)
        (args.out / out_name).write_bytes(blob)
        print(f"{out_name}: {items} items, {points} points, {len(blob)} bytes")


if __name__ == "__main__":
    main()
