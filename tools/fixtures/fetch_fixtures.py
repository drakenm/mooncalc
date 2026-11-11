#!/usr/bin/env python3
"""Re-fetch the API-based validation fixtures in tests/fixtures/.

Standard library only. Run from the repository root:

    python -I tools/fixtures/fetch_fixtures.py all
    python -I tools/fixtures/fetch_fixtures.py horizons-geo horizons-topo
    python -I tools/fixtures/fetch_fixtures.py --list

Each target overwrites its output CSV(s). Requests are sequential with a
pause between them, and send only a generic User-Agent. See
docs/validation.md for what each fixture contains and its terms of use.

Targets:
    horizons-geo    JPL Horizons geocentric apparent Sun and Moon (TT epochs)
    horizons-topo   JPL Horizons topocentric alt/az, airless and refracted (UT epochs)
    usno-rstt       USNO rise/set/transit/twilight, one UT day per row group
    usno-phases     USNO primary Moon phases 2000-2030 plus a few historical years
    deltat          USNO/IERS observed TT-UT1 and NASA (Espenak) historical tables
    eclipses        Selected eclipses from NASA's Five Millennium catalogs
    hijri           Umm al-Qura principal days and Saudi announced month starts (van Gent)
"""
import csv
import html
import json
import os
import re
import sys
import time
import urllib.parse
import urllib.request
from datetime import date, datetime, timedelta, timezone

USER_AGENT = "mooncalc-validation-fixtures"
PAUSE_S = 1.5
ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
FIX = os.path.join(ROOT, "tests", "fixtures")

# Observer sites used by the topocentric and rise/set fixtures. These are
# test inputs (approximate city coordinates), not reference data.
# name: (east longitude deg, latitude deg, height m)
SITES = {
    "mecca": (39.8262, 21.4225, 277.0),
    "greenwich": (-0.0015, 51.4769, 46.0),
    "cape_town": (18.4241, -33.9249, 10.0),
    "kuala_lumpur": (101.6869, 3.1390, 60.0),
    "los_angeles": (-118.2437, 34.0522, 90.0),
    "tromso": (18.9560, 69.6496, 10.0),
}


def http_get(url):
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    for attempt in range(3):
        try:
            with urllib.request.urlopen(req, timeout=90) as resp:
                body = resp.read()
            time.sleep(PAUSE_S)
            return body
        except Exception as exc:  # noqa: BLE001 - report and retry politely
            print(f"  retry {attempt + 1} after error: {exc}", file=sys.stderr)
            time.sleep(10 * (attempt + 1))
    raise RuntimeError(f"failed to fetch {url}")


def write_csv(relpath, header, rows):
    path = os.path.join(FIX, relpath)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", newline="\n", encoding="utf-8") as fh:
        w = csv.writer(fh, lineterminator="\n")
        w.writerow(header)
        w.writerows(rows)
    print(f"wrote {relpath}: {len(rows)} rows")


def jd_from_datetime(dt):
    """Julian Day of a proleptic Gregorian datetime (naive, in whatever scale)."""
    days = (dt - datetime(2000, 1, 1, 12, 0, 0)).total_seconds() / 86400.0
    return 2451545.0 + days


# --------------------------------------------------------------------------
# JPL Horizons
# --------------------------------------------------------------------------
HORIZONS = "https://ssd.jpl.nasa.gov/api/horizons.api"


def geo_epochs_tt():
    """~44 TT epochs over 1600-2200, denser over 1900-2100.

    The day of year and hour are varied deterministically so the epochs are
    not all at 0h on 1 January.
    """
    years = [1600, 1650, 1700, 1750, 1800, 1850, 1875]
    years += list(range(1900, 2000, 8)) + [1996, 2000, 2004, 2008, 2012, 2016, 2020, 2024, 2028]
    years += list(range(2036, 2101, 8)) + [2125, 2150, 2175, 2200]
    years = sorted(set(years))
    out = []
    for i, y in enumerate(years):
        dt = datetime(y, 1, 1) + timedelta(days=(i * 37) % 365, hours=(i * 5) % 24, minutes=(i * 13) % 60)
        out.append(round(jd_from_datetime(dt), 6))
    # Extra epochs: J2000.0 and the epochs of Meeus (1998) examples 25.a and 47.a.
    out += [2451545.0, 2448908.5, 2448724.5]
    return sorted(set(out))


def horizons_query(params):
    base = {
        "format": "text",
        "MAKE_EPHEM": "YES",
        "EPHEM_TYPE": "OBSERVER",
        "OBJ_DATA": "NO",
        "ANG_FORMAT": "DEG",
        "EXTRA_PREC": "YES",
        "CSV_FORMAT": "YES",
        "CAL_FORMAT": "BOTH",
        "TLIST_TYPE": "JD",
    }
    base.update(params)
    url = HORIZONS + "?" + urllib.parse.urlencode(base)
    text = http_get(url).decode("utf-8")
    if "$$SOE" not in text:
        raise RuntimeError("Horizons returned no ephemeris:\n" + text[-2000:])
    head, rest = text.split("$$SOE", 1)
    body = rest.split("$$EOE", 1)[0]
    # The CSV header is the last non-rule line before $$SOE.
    hdr_line = [l for l in head.splitlines() if l.strip() and not l.startswith("*")][-1]
    cols = [c.strip() for c in hdr_line.split(",")]
    rows = []
    for line in body.strip().splitlines():
        vals = [v.strip() for v in line.split(",")]
        rows.append(dict(zip(cols, vals)))
    meta = {}
    for key in ("Target body name", "Atmos refraction", "EOP file", "Center geodetic"):
        m = re.search(rf"^{re.escape(key)}\s*:\s*(.+)$", head, re.M)
        if m:
            meta[key] = m.group(1).strip()
    return rows, meta


def pick(row, prefix):
    for k, v in row.items():
        if k.startswith(prefix):
            return v
    raise KeyError(prefix)


def tlist(jds):
    return " ".join(f"'{jd:.6f}'" for jd in jds)


def fetch_horizons_geo():
    epochs = geo_epochs_tt()
    out = []
    for target, name in (("10", "sun"), ("301", "moon")):
        rows, meta = horizons_query({
            "COMMAND": f"'{target}'",
            "CENTER": "'500@399'",
            "TIME_TYPE": "TT",
            "TLIST": tlist(epochs),
            "QUANTITIES": "'2,10,13,20,23,30,31'",
        })
        print(f"  {name}: {meta}")
        for r in rows:
            delta_au = float(pick(r, "delta"))
            out.append([
                name,
                pick(r, "Date_________JDTT"),
                pick(r, "Date__(TT)"),
                pick(r, "TDB-UT"),
                pick(r, "R.A._"),
                pick(r, "DEC_"),
                pick(r, "ObsEcLon"),
                pick(r, "ObsEcLat"),
                f"{delta_au:.14f}",
                f"{delta_au * 149597870.700:.3f}",
                pick(r, "Ang-diam"),
                pick(r, "Illu%"),
                pick(r, "S-O-T"),
            ])
    write_csv("ephemeris/horizons_geocentric.csv", [
        "body", "jd_tt", "calendar_tt", "tdb_minus_ut_s", "ra_app_deg", "dec_app_deg",
        "ecl_lon_app_deg", "ecl_lat_app_deg", "delta_au", "delta_km", "ang_diam_arcsec",
        "illum_pct", "elong_sot_deg",
    ], out)


def topo_epochs_ut(site_lon):
    """UT epochs for one site: two evening-crescent dates near local sunset
    (approximate local mean time 18:30) plus three generic instants."""
    out = []
    for d in (date(2024, 3, 11), date(2024, 4, 9), date(2025, 3, 30)):
        lmt = datetime(d.year, d.month, d.day, 18, 30)
        out.append(jd_from_datetime(lmt - timedelta(hours=site_lon / 15.0)))
    for dt in (datetime(2000, 1, 1, 12, 0), datetime(2015, 6, 15, 6, 0), datetime(2030, 9, 1, 21, 30)):
        out.append(jd_from_datetime(dt))
    return [round(x, 6) for x in sorted(out)]


def fetch_horizons_topo():
    out = []
    for site in ("mecca", "greenwich", "cape_town", "kuala_lumpur", "los_angeles"):
        lon, lat, h_m = SITES[site]
        epochs = topo_epochs_ut(lon)
        for target, name in (("10", "sun"), ("301", "moon")):
            res = {}
            for mode in ("AIRLESS", "REFRACTED"):
                rows, meta = horizons_query({
                    "COMMAND": f"'{target}'",
                    "CENTER": "'coord@399'",
                    "COORD_TYPE": "GEODETIC",
                    "SITE_COORD": f"'{lon},{lat},{h_m / 1000.0}'",
                    "TIME_TYPE": "UT",
                    "TLIST": tlist(epochs),
                    "QUANTITIES": "'2,4,13,20'",
                    "APPARENT": mode,
                })
                print(f"  {site} {name} {mode}: {meta.get('Atmos refraction')}")
                res[mode] = rows
            for a, r in zip(res["AIRLESS"], res["REFRACTED"]):
                assert pick(a, "Date_________JDUT") == pick(r, "Date_________JDUT")
                delta_au = float(pick(a, "delta"))
                out.append([
                    site, f"{lon}", f"{lat}", f"{h_m:.0f}", name,
                    pick(a, "Date_________JDUT"), pick(a, "Date__(UT)"),
                    pick(a, "R.A._"), pick(a, "DEC_"),
                    pick(a, "Azi"), pick(a, "Elev"),
                    pick(r, "Azi"), pick(r, "Elev"),
                    f"{delta_au * 149597870.700:.3f}",
                    pick(a, "Ang-diam"),
                ])
    write_csv("ephemeris/horizons_topocentric.csv", [
        "site", "lon_deg", "lat_deg", "height_m", "body", "jd_ut", "calendar_ut",
        "ra_app_topo_deg", "dec_app_topo_deg", "az_airless_deg", "alt_airless_deg",
        "az_refracted_deg", "alt_refracted_deg", "delta_topo_km", "ang_diam_arcsec",
    ], out)


# --------------------------------------------------------------------------
# USNO Astronomical Applications API
# --------------------------------------------------------------------------
USNO = "https://aa.usno.navy.mil/api"

RSTT_DATES = [
    "2024-01-15", "2024-03-20", "2024-06-21", "2024-09-22", "2024-12-21",
    "2025-02-28", "2025-05-10", "2025-08-05", "2025-11-01", "2026-01-20",
]
RSTT_EXTRA = {"tromso": ["2024-05-20", "2024-07-25", "2024-11-25", "2025-01-17"]}

PHEN_CODES = {
    "Begin Civil Twilight": "civil_twilight_begin",
    "End Civil Twilight": "civil_twilight_end",
    "Rise": "rise",
    "Set": "set",
    "Upper Transit": "upper_transit",
    "Lower Transit": "lower_transit",
}


def note_code(phen):
    """'Object continuously above the Horizon' -> 'continuously_above_horizon'."""
    words = re.sub(r"[^a-z ]", "", phen.lower()).split()
    return "_".join(w for w in words if w not in ("object", "the"))


def fetch_usno_rstt():
    out = []
    for site, (lon, lat, _h) in SITES.items():
        for d in RSTT_DATES + RSTT_EXTRA.get(site, []):
            q = urllib.parse.urlencode({"date": d, "coords": f"{lat},{lon}", "tz": "0"})
            data = json.loads(http_get(f"{USNO}/rstt/oneday?{q}"))
            props = data["properties"]["data"]
            for body, key in (("sun", "sundata"), ("moon", "moondata")):
                for ph in props.get(key, []):
                    phen = ph["phen"]
                    code = PHEN_CODES.get(phen) or note_code(phen)
                    out.append([site, f"{lon}", f"{lat}", d, body, code, ph.get("time") or "", phen])
            out.append([site, f"{lon}", f"{lat}", d, "moon", "fracillum", props.get("fracillum", ""),
                        props.get("curphase", "")])
    write_csv("riseset/usno_rstt.csv", [
        "site", "lon_deg", "lat_deg", "date_ut", "body", "event", "time_ut", "usno_phen",
    ], out)


PHASE_YEARS = list(range(2000, 2031)) + [1700, 1800, 1900]


def fetch_usno_phases():
    out = []
    for y in sorted(PHASE_YEARS):
        data = json.loads(http_get(f"{USNO}/moon/phases/year?year={y}"))
        for p in data["phasedata"]:
            out.append([f"{p['year']:04d}-{p['month']:02d}-{p['day']:02d}", p["time"], p["phase"]])
    write_csv("phases/usno_moon_phases.csv", ["date_ut", "time_ut", "phase"], out)


# --------------------------------------------------------------------------
# Delta T
# --------------------------------------------------------------------------
def fetch_deltat():
    rows = []
    hist = http_get("https://maia.usno.navy.mil/ser7/historic_deltat.data").decode("ascii", "replace")
    for line in hist.splitlines():
        parts = line.split()
        if len(parts) >= 3 and re.match(r"^\d{4}\.\d+$", parts[0]):
            y = float(parts[0])
            if y < 1973.0:
                rows.append([parts[0], parts[1], parts[2], "usno_historic_deltat"])
    monthly = http_get("https://maia.usno.navy.mil/ser7/deltat.data").decode("ascii", "replace")
    for line in monthly.splitlines():
        parts = line.split()
        if len(parts) == 4 and parts[0].isdigit():
            y, m, d = int(parts[0]), int(parts[1]), int(parts[2])
            if d == 1 and m == 1:  # keep annual (1 January) values
                rows.append([f"{y}.000", parts[3], "", "usno_deltat_monthly"])
    write_csv("deltat/usno_observed_deltat.csv", ["year", "delta_t_s", "error_s", "source"], rows)

    page = http_get("https://eclipse.gsfc.nasa.gov/SEhelp/deltat2004.html").decode("latin-1")
    t1, t2 = [], []
    for tab in re.findall(r"<table.*?</table>", page, re.S | re.I):
        for tr in re.findall(r"<tr.*?</tr>", tab, re.S | re.I):
            cells = [html.unescape(re.sub(r"<[^>]*>", "", c)).strip()
                     for c in re.findall(r"<t[dh][^>]*>(.*?)</t[dh]>", tr, re.S | re.I)]
            if len(cells) == 3 and re.match(r"^-?\d+$", cells[0]):
                t1.append(cells)
            elif len(cells) == 4 and re.match(r"^\d{4}\.0$", cells[0]):
                t2.append([cells[0], cells[1].lstrip("+")])
    write_csv("deltat/nasa_deltat_historical.csv", ["year", "delta_t_s", "std_error_s"], t1)
    write_csv("deltat/nasa_deltat_recent.csv", ["year", "delta_t_s"], t2)


# --------------------------------------------------------------------------
# NASA Five Millennium eclipse catalogs
# --------------------------------------------------------------------------
SOLAR_PICKS = {
    "1901-2000": ["1919 May 29", "1973 Jun 30", "1999 Aug 11"],
    "2001-2100": ["2001 Jun 21", "2017 Aug 21", "2022 Oct 25", "2023 Apr 20", "2023 Oct 14",
                  "2024 Apr 08", "2024 Oct 02", "2025 Mar 29", "2026 Aug 12", "2027 Aug 02"],
}
LUNAR_PICKS = {
    "1901-2000": ["1982 Jul 06", "2000 Jul 16"],
    "2001-2100": ["2018 Jul 27", "2019 Jan 21", "2022 Nov 08", "2023 May 05", "2023 Oct 28",
                  "2024 Mar 25", "2025 Mar 14", "2025 Sep 07", "2026 Mar 03"],
}
MONTHS = {m: i for i, m in enumerate(
    ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"], 1)}


def catalog_lines(url):
    page = http_get(url).decode("latin-1")
    text = html.unescape(re.sub(r"<[^>]*>", "", page))
    return text.splitlines()


def fetch_eclipses():
    solar = []
    for rng, picks in SOLAR_PICKS.items():
        lines = catalog_lines(f"https://eclipse.gsfc.nasa.gov/SEcat5/SE{rng}.html")
        for p in picks:
            line = next(l for l in lines if re.match(rf"^\d{{5}}\s+{p}\s", l))
            f = line.split()
            # cat, y, mon, d, td, dT, luna, saros, type, qle, gamma, mag, [lat, lon, alt, width, dur]
            y, mon, d = int(f[1]), MONTHS[f[2]], int(f[3])
            path = f[12:] if len(f) > 12 else []
            solar.append([f[0], f"{y:04d}-{mon:02d}-{d:02d}", f[4], f[5], f[6], f[7], f[8], f[9], f[10], f[11],
                          path[0] if len(path) > 0 else "", path[1] if len(path) > 1 else "",
                          path[2] if len(path) > 2 else "", path[3] if len(path) > 3 else "",
                          path[4] if len(path) > 4 else ""])
    write_csv("eclipses/nasa_solar_eclipses.csv", [
        "cat_no", "date_td", "greatest_td", "delta_t_s", "lunation", "saros", "type", "qle",
        "gamma", "magnitude", "lat", "lon", "sun_alt_deg", "path_width_km", "central_duration",
    ], solar)

    lunar = []
    for rng, picks in LUNAR_PICKS.items():
        lines = catalog_lines(f"https://eclipse.gsfc.nasa.gov/LEcat5/LE{rng}.html")
        for p in picks:
            line = next(l for l in lines if re.match(rf"^\d{{5}}\s+{p}\s", l))
            f = line.split()
            # cat, y, mon, d, td, dT, luna, saros, type, qse, gamma, pen_mag, um_mag, pen, par, total, lat, lon
            y, mon, d = int(f[1]), MONTHS[f[2]], int(f[3])
            lunar.append([f[0], f"{y:04d}-{mon:02d}-{d:02d}"] + f[4:18])
    write_csv("eclipses/nasa_lunar_eclipses.csv", [
        "cat_no", "date_td", "greatest_td", "delta_t_s", "lunation", "saros", "type", "qse",
        "gamma", "pen_magnitude", "um_magnitude", "dur_pen_min", "dur_par_min", "dur_total_min",
        "zenith_lat", "zenith_lon",
    ], lunar)


# --------------------------------------------------------------------------
# Umm al-Qura (R.H. van Gent, Utrecht University)
# --------------------------------------------------------------------------
VAN_GENT = "https://webspace.science.uu.nl/~gent0113/islam/"


def html_rows(page):
    out = []
    for tr in re.findall(r"<tr.*?</tr>", page, re.S | re.I):
        cells = re.findall(r"<t[dh][^>]*>(.*?)</t[dh]>", tr, re.S | re.I)
        out.append([html.unescape(re.sub(r"<[^>]*>", "", c)).replace("\xa0", " ").strip() for c in cells])
    return out


def iso(dmy):
    """'26 Mar 2001' -> '2001-03-26'; '' stays ''."""
    if not dmy:
        return ""
    d, mon, y = dmy.split()
    return f"{int(y):04d}-{MONTHS[mon]:02d}-{int(d):02d}"


def fetch_hijri():
    page = http_get(VAN_GENT + "ummalqura_principal.htm").decode("utf-8", "replace")
    rows = []
    for cells in html_rows(page):
        if cells and re.match(r"^14\d\d$", cells[0]):
            # year, then (date, weekday) pairs for 5 observances
            dates = [iso(c) for c in cells[1:] if re.match(r"^\d{1,2} [A-Z][a-z]{2} \d{4}$", c)]
            assert len(dates) == 5, cells
            rows.append([cells[0]] + dates)
    write_csv("hijri/ummalqura_principal_days.csv", [
        "hijri_year", "muharram_1", "rabi_al_awwal_12", "ramadan_1", "shawwal_1", "dhul_hijja_10",
    ], rows)

    page = http_get(VAN_GENT + "ummalqura_adjust.htm").decode("utf-8", "replace")
    labels = [("1", "1"), ("9", "1"), ("10", "1"), ("12", "1"), ("12", "10")]
    rows = []
    for cells in html_rows(page):
        if cells and re.match(r"^14\d\d$", cells[0]) and len(cells) == 11:
            for i, (month, day) in enumerate(labels):
                computed, announced = cells[1 + 2 * i], cells[2 + 2 * i]
                # In the source a blank 'announced' cell next to a 1 Muharram
                # value for 1422-1430 means no announcement is tabulated.
                rows.append([cells[0], month, day, iso(computed), iso(announced)])
    write_csv("hijri/saudi_announced_vs_ummalqura.csv", [
        "hijri_year", "hijri_month", "hijri_day", "ummalqura_gregorian", "announced_gregorian",
    ], rows)


TARGETS = {
    "horizons-geo": fetch_horizons_geo,
    "horizons-topo": fetch_horizons_topo,
    "usno-rstt": fetch_usno_rstt,
    "usno-phases": fetch_usno_phases,
    "deltat": fetch_deltat,
    "eclipses": fetch_eclipses,
    "hijri": fetch_hijri,
}


def main(argv):
    if not argv or argv == ["--list"]:
        print("targets: all " + " ".join(TARGETS))
        return
    names = list(TARGETS) if argv == ["all"] else argv
    for n in names:
        if n not in TARGETS:
            sys.exit(f"unknown target {n!r}; use --list")
    print(f"retrieved {datetime.now(timezone.utc):%Y-%m-%d} UTC")
    for n in names:
        print(f"== {n}")
        TARGETS[n]()


if __name__ == "__main__":
    main(sys.argv[1:])
