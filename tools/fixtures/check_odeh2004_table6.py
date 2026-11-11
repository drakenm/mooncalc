#!/usr/bin/env python3
"""Consistency checks for the hand-transcribed Odeh (2004) Table VI fixture.

    python -I tools/fixtures/check_odeh2004_table6.py tests/fixtures/crescent/odeh2004_table6.csv

Recomputes V from ARCV and W (Odeh eq. 2), ARCL from ARCV and DAZ, the local
solar date from the best-time JD and longitude, the age sign against E/M, and
the ascending order of V within the table. Standard library only. Flags are
printed; see docs/validation.md for the known printed inconsistencies.
"""
import csv, math, sys
COLS = "no,ref,me,date,observer,lon_deg,lat_deg,elev_m,naked,binoc,tele,jd_best,age_h,lag_min,arcv_deg,daz_deg,arcl_deg,w_arcsec,v,group,page".split(",")
def jd_to_date(jd):
    z = int(jd + 0.5); f = jd + 0.5 - z
    if z >= 2299161:
        a = int((z - 1867216.25) / 36524.25); a = z + 1 + a - a // 4
    else:
        a = z
    b = a + 1524; c = int((b - 122.1) / 365.25); d = int(365.25 * c); e = int((b - d) / 30.6001)
    day = b - d - int(30.6001 * e); m = e - 1 if e < 14 else e - 13; y = c - 4716 if m > 2 else c - 4715
    return y, m, day
bad = 0; n = 0
for path in sys.argv[1:]:
    with open(path, newline="", encoding="utf-8") as fh:
        rows = [r for r in csv.reader(fh) if r and not r[0].startswith("#")]
    if rows and rows[0][0] == "no": rows = rows[1:]
    prev_v = None
    for r in rows:
        n += 1
        msgs = []
        if len(r) != len(COLS):
            print(f"{path}: row {r[:1]} has {len(r)} fields"); bad += 1; continue
        d = dict(zip(COLS, r))
        try:
            arcv, daz, arcl, w, v = (float(d[k]) for k in ("arcv_deg", "daz_deg", "arcl_deg", "w_arcsec", "v"))
            jd, age, lon = float(d["jd_best"]), float(d["age_h"]), float(d["lon_deg"])
            float(d["lat_deg"]); float(d["lag_min"]); int(d["elev_m"]); int(d["no"])
        except ValueError as e:
            print(f"{path}: no={d['no']} parse error {e}"); bad += 1; continue
        W = w / 60.0
        vc = arcv - (-0.1018 * W**3 + 0.7319 * W**2 - 6.3226 * W + 7.1651)
        if abs(vc - v) > 0.12: msgs.append(f"V recomputed {vc:.2f} vs {v}")
        ac = math.degrees(math.acos(math.cos(math.radians(arcv)) * math.cos(math.radians(daz))))
        if abs(ac - arcl) > 0.35: msgs.append(f"ARCL recomputed {ac:.2f} vs {arcl}")
        if d["me"] not in ("E", "M"): msgs.append("E/M field")
        if (d["me"] == "M") != (age < 0): msgs.append("age sign vs E/M")
        if d["ref"] not in tuple("ABCDFI"): msgs.append("ref code")
        for k in ("naked", "binoc", "tele"):
            if d[k] not in ("", "I", "V"): msgs.append(f"{k} code")
        dd, mm, yy = (int(x) for x in d["date"].split("-"))
        loc = jd_to_date(jd + lon / 360.0)
        if loc != (yy, mm, dd): msgs.append(f"local-solar date from JD {loc} vs {d['date']}")
        if prev_v is not None and v < prev_v - 1e-9: msgs.append(f"V not ascending (prev {prev_v})")
        prev_v = v
        if msgs:
            bad += 1; print(f"{path}: no={d['no']}: " + "; ".join(msgs))
print(f"checked {n} rows, {bad} flagged")
