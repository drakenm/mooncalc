#!/usr/bin/env python3
"""Extract Table 4 (295 first-sighting records) from Yallop (1997), NAO TN 69.

Source PDF: https://www.astronomycenter.net/pdf/yallop_1997.pdf
Convert it to text first with poppler's pdftotext in raw mode:

    pdftotext -raw -enc UTF-8 yallop_1997.pdf yallop.txt
    python -I tools/fixtures/extract_yallop_table4.py yallop.txt tests/fixtures/crescent/yallop1997_table4.csv

Standard library only. The script also runs internal consistency checks
(recomputed q from ARCV and W', ARCL from ARCV and DAZ, record count) and
prints any row that fails them.
"""
import csv
import math
import re
import sys

ROW = re.compile(
    r"^(\d{1,3}) (\d{4}) (\d{1,2}) (\d{1,2}) ([EM]) "
    r"(\S+) (\S+) (\S+) (\S+) (\S+) (\S+) (\S+) (\S+) (\S+) (\S+) (\S+) (.+)$"
)
CATEGORY = re.compile(r"^\(([A-F])\)")
COLUMNS = [
    "no", "year", "month", "day", "me", "jd_newmoon", "lat_deg", "lon_deg",
    "arcl_deg", "arcv_deg", "daz_deg", "age_h", "lag_min", "parallax_arcmin",
    "w_topo_arcmin", "q", "vis_observed", "vis_predicted", "q_category",
]


def num(s):
    """Normalise the PDF's typography: middle dot decimal, Unicode minus/en dash."""
    return s.replace("·", ".").replace("−", "-").replace("–", "-")


VIS = re.compile(r"^([VI](?:\([A-Z]\))?)\s*([VI](?:\([A-Z]\))?)$")


def split_vis(tail):
    """Split the BES and BDY codes; the PDF sometimes prints them without a space."""
    m = VIS.match(tail.strip())
    if not m:
        raise ValueError(f"cannot split visibility codes from {tail!r}")
    return [m.group(1), m.group(2)]


def main(src, dst):
    lines = open(src, encoding="utf-8").read().splitlines()
    start = next(i for i, l in enumerate(lines) if l.startswith("Table 4: The 295 observations"))
    end = next(i for i, l in enumerate(lines) if l.startswith("6. Derivation of the q-test"))
    rows, cat = [], None
    for line in lines[start:end]:
        m = CATEGORY.match(line)
        if m:
            cat = m.group(1)
            continue
        m = ROW.match(line)
        if not m:
            continue
        g = [num(x) for x in m.groups()]
        obs, pred = split_vis(g[16])
        jd = float(g[5]) + 2400000.0
        rows.append(g[:5] + [f"{jd:.3f}"] + g[6:16] + [obs, pred, cat])

    problems = 0
    for r in rows:
        d = dict(zip(COLUMNS, r))
        arcv, daz, arcl = float(d["arcv_deg"]), float(d["daz_deg"]), float(d["arcl_deg"])
        w, q = float(d["w_topo_arcmin"]), float(d["q"])
        qc = (arcv - (11.8371 - 6.3226 * w + 0.7319 * w**2 - 0.1018 * w**3)) / 10.0
        ac = math.degrees(math.acos(math.cos(math.radians(arcv)) * math.cos(math.radians(daz))))
        msgs = []
        # W' is printed to 2 significant figures and ARCV to 0.1 deg, so the
        # tolerance on q follows from their rounding.
        decimals = len(d["w_topo_arcmin"].split(".")[1])
        dw = 0.5 * 10 ** -decimals
        slope = abs(-6.3226 + 2 * 0.7319 * w - 3 * 0.1018 * w**2)
        if abs(qc - q) > (0.05 + slope * dw) / 10.0 + 0.0006:
            msgs.append(f"q recomputed {qc:.3f}")
        # Equation (2.1) is the small-angle relation; only check it below 22 deg.
        if arcl < 22.0 and abs(ac - arcl) > 0.15:
            msgs.append(f"ARCL recomputed {ac:.2f}")
        if msgs:
            problems += 1
            print(f"no {d['no']}: " + "; ".join(msgs), file=sys.stderr)
    nos = [int(r[0]) for r in rows]
    print(f"{len(rows)} rows, {len(set(nos))} distinct record numbers, {problems} flagged", file=sys.stderr)
    with open(dst, "w", newline="\n", encoding="utf-8") as fh:
        w = csv.writer(fh, lineterminator="\n")
        w.writerow(COLUMNS)
        w.writerows(rows)


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
