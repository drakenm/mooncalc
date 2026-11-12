#!/usr/bin/env python3
"""Convert GeoNames dump files into MoonCalc's embedded city atlas.

Usage (from the repository root):

    python -I tools/atlas/build_atlas.py --download          # fetch, then convert
    python -I tools/atlas/build_atlas.py --input DIR         # convert existing files

Inputs (https://download.geonames.org/export/dump/, CC BY 4.0):
    citiesNNNNN.zip, countryInfo.txt, admin1CodesASCII.txt, timeZones.txt

Output: assets/atlas/cities.tsv (UTF-8, LF). The format is documented in
tools/atlas/README.md and parsed by crates/mooncalc-atlas/src/builtin.rs.
Uses only the Python standard library.
"""

import argparse
import datetime
import io
import os
import sys
import unicodedata
import urllib.request
import zipfile

BASE_URL = "https://download.geonames.org/export/dump/"
USER_AGENT = "Mozilla/5.0 (compatible; atlas-data-fetch)"
FILES = ["countryInfo.txt", "admin1CodesASCII.txt", "timeZones.txt"]

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
DEFAULT_DOWNLOAD = os.path.join(HERE, "download")
DEFAULT_OUTPUT = os.path.join(REPO, "assets", "atlas", "cities.tsv")

# Alternate names are only kept for cities at least this populous, which keeps
# the embedded file small while making well-known exonyms (Mecca, Munich,
# Cologne, Peking ...) searchable.
ALT_MIN_POPULATION = 50_000
MAX_ALTS = 12


def download(dest, cities):
    os.makedirs(dest, exist_ok=True)
    for name in [cities + ".zip"] + FILES:
        url = BASE_URL + name
        print("downloading", url, file=sys.stderr)
        req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
        with urllib.request.urlopen(req, timeout=120) as resp:
            data = resp.read()
        with open(os.path.join(dest, name), "wb") as f:
            f.write(data)


def read_lines(path):
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.rstrip("\n").rstrip("\r")
            if line and not line.startswith("#"):
                yield [field.strip() for field in line.split("\t")]


def fold(s):
    """Lower-case and strip diacritics (used only for de-duplication)."""
    s = unicodedata.normalize("NFKD", s.lower())
    return "".join(c for c in s if not unicodedata.combining(c))


def is_latin(s):
    for c in s:
        if c.isalpha():
            try:
                if "LATIN" not in unicodedata.name(c):
                    return False
            except ValueError:
                return False
        elif c.isdigit():
            return False
    return True


def pick_alts(name, ascii_name, alts):
    seen = {fold(name), fold(ascii_name)}
    out = []
    for a in alts.split(","):
        a = a.strip()
        if not a or "\t" in a or "|" in a:
            continue
        if not a[0].isupper() or not is_latin(a):
            continue  # lower-case romanisations and non-Latin scripts
        if len(a) <= 4 and a.isupper():
            continue  # airport / ISO codes
        k = fold(a)
        if k in seen:
            continue
        seen.add(k)
        out.append(a)
    # Shortest first: these are usually the common exonyms.
    out.sort(key=lambda x: (len(x), x))
    return out[:MAX_ALTS]


def fmt_num(x, places):
    s = ("%.*f" % (places, float(x))).rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--input", default=DEFAULT_DOWNLOAD, help="directory with the GeoNames files")
    ap.add_argument("--output", default=DEFAULT_OUTPUT)
    ap.add_argument("--cities", default="cities15000", help="cities15000, cities5000, ...")
    ap.add_argument("--download", action="store_true", help="download the files into --input first")
    ap.add_argument("--no-alts", action="store_true", help="omit alternate names")
    args = ap.parse_args()

    if args.download:
        download(args.input, args.cities)

    countries = {}  # code -> (iso3, name)
    for r in read_lines(os.path.join(args.input, "countryInfo.txt")):
        countries[r[0]] = (r[1], r[4])

    admin1 = {}  # "CC.code" -> name
    for r in read_lines(os.path.join(args.input, "admin1CodesASCII.txt")):
        admin1[r[0]] = r[1]

    tzs = {}  # id -> (standard, daylight) offsets in hours
    for r in read_lines(os.path.join(args.input, "timeZones.txt")):
        if r[0] == "CountryCode":
            continue
        jan, jul, raw = float(r[2]), float(r[3]), float(r[4])
        # The DST offset is whichever of the January/July offsets differs from
        # the raw (standard) offset; equal to it where DST is not observed.
        dst = jul if jul != raw else jan
        tzs[r[1]] = (raw, dst)

    with zipfile.ZipFile(os.path.join(args.input, args.cities + ".zip")) as z:
        text = z.read(args.cities + ".txt").decode("utf-8")

    rows = []
    for line in text.splitlines():
        r = [field.strip() for field in line.split("\t")]
        if len(r) < 19:
            continue
        name, ascii_name, alts = r[1].strip(), r[2].strip(), r[3]
        lat, lon = r[4], r[5]
        cc, a1 = r[8], r[10]
        pop = int(r[14] or 0)
        elev = r[15].strip()
        dem = r[16].strip()
        tz = r[17]
        if elev:
            e = int(round(float(elev)))
        elif dem and int(float(dem)) > -9000:
            e = int(round(float(dem)))
        else:
            e = 0
        if cc not in countries:
            print("skip (unknown country %r): %s" % (cc, name), file=sys.stderr)
            continue
        if tz not in tzs:
            print("skip (unknown time zone %r): %s" % (tz, name), file=sys.stderr)
            continue
        if not ascii_name:
            ascii_name = name
        alt_list = []
        if not args.no_alts and pop >= ALT_MIN_POPULATION:
            alt_list = pick_alts(name, ascii_name, alts)
        rows.append((name, ascii_name, alt_list, cc, admin1.get(cc + "." + a1, ""),
                     lat, lon, e, pop, tz))

    # Stable order: country, then population descending, then name.
    rows.sort(key=lambda x: (x[3], -x[8], x[1], x[0]))

    used_cc = sorted({x[3] for x in rows})
    cc_idx = {c: i for i, c in enumerate(used_cc)}
    used_a1 = sorted({(x[3], x[4]) for x in rows if x[4]})
    a1_idx = {k: i for i, k in enumerate(used_a1)}
    used_tz = sorted({x[9] for x in rows})
    tz_idx = {t: i for i, t in enumerate(used_tz)}

    for x in rows:
        for s in (x[0], x[1], x[4]):
            if "\t" in s or "\n" in s:
                sys.exit("field contains a tab/newline: %r" % s)

    out = io.StringIO()
    w = out.write
    w("#mooncalc-atlas\t1\n")
    w("#source\tGeoNames %s (https://www.geonames.org/), CC BY 4.0\n" % args.cities)
    w("#generated\t%s\n" % datetime.date.today().isoformat())
    w("@countries\t%d\n" % len(used_cc))
    for c in used_cc:
        w("%s\t%s\t%s\n" % (c, countries[c][0], countries[c][1]))
    w("@admin1\t%d\n" % len(used_a1))
    for cc, n in used_a1:
        w("%d\t%s\n" % (cc_idx[cc], n))
    w("@timezones\t%d\n" % len(used_tz))
    for t in used_tz:
        std, dst = tzs[t]
        w("%s\t%s\t%s\n" % (t, fmt_num(std, 2), fmt_num(dst, 2)))
    w("@cities\t%d\n" % len(rows))
    for name, asc, alts, cc, a1, lat, lon, e, pop, tz in rows:
        w("\t".join([
            name,
            "" if asc == name else asc,
            "|".join(alts),
            str(cc_idx[cc]),
            str(a1_idx[(cc, a1)]) if a1 else "",
            fmt_num(lat, 4),
            fmt_num(lon, 4),
            str(e),
            str(pop),
            str(tz_idx[tz]),
        ]) + "\n")

    data = out.getvalue().encode("utf-8")
    os.makedirs(os.path.dirname(os.path.abspath(args.output)), exist_ok=True)
    with open(args.output, "wb") as f:
        f.write(data)
    print("wrote %s: %d cities, %d bytes" % (args.output, len(rows), len(data)), file=sys.stderr)


if __name__ == "__main__":
    main()
