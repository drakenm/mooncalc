#!/usr/bin/env python3
"""Extract the per-city table from an HMNAO "Crescent Moon Visibility summary" PDF.

HM Nautical Almanac Office publishes a monthly summary on GOV.UK, e.g.
https://www.gov.uk/government/publications/crescent-moon-visibility-for-march-2026
Convert the summary PDF to text first with poppler's pdftotext in raw mode:

    pdftotext -raw -enc UTF-8 Crescent_Moon_Visibility_summary_for_March_2026.pdf mar.txt
    python -I tools/fixtures/extract_hmnao_summary.py 2026 3 mar.txt [more year month file ...] \
        > tests/fixtures/predictions/hmnao_city_visibility.csv

Standard library only. Times are local civil times as printed, with the UTC
offset given per city; this script also converts them to UT.
"""
import csv
import re
import sys
from datetime import date, datetime, timedelta

# HMNAO phrases (as in its Yallop-based MoonWatch products) -> Yallop category.
CATEGORIES = {
    "easily visible": "A",
    "visible in perfect conditions": "B",
    "may need optical aid": "C",
    "will need optical aid": "D",
    "not visible with a conventional telescope": "E",
    "not visible": "F",
}
CATEGORY_STARTS = ("easily", "visible", "may", "will", "not")


def parse(year, month, path):
    lines = [l.strip() for l in open(path, encoding="utf-8").read().splitlines() if l.strip()]
    start = next(i for i, l in enumerate(lines) if l.startswith("Visibility of the New Crescent Moon in "))
    end = next(i for i, l in enumerate(lines) if l.startswith("New Crescent Moon Visibility Maps for"))
    body = lines[start + 1:end]
    first_loc = next(i for i, l in enumerate(body) if l.startswith("Times in")) - 1
    days = [int(m) for m in re.findall(r"\b(\d{1,2})(?:st|nd|rd|th)\b", " ".join(body[:first_loc]))]
    rows, i = [], first_loc
    while i < len(body):
        name_parts = []
        while not body[i].startswith("Times in"):
            name_parts.append(body[i])
            i += 1
        zone = body[i].replace("Times in", "").strip()
        m = re.match(r"i\.e\. UTC([+-−]\d+(?:\.\d+)?)hr", body[i + 1].replace("−", "-"))
        offset_h = float(m.group(1))
        i += 2
        cells = []
        while len(cells) < len(days) or (i < len(body) and body[i].startswith(("SS:", "MS:"))):
            l = body[i]
            if l.startswith(("SS:", "MS:")):
                cells[-1][l[:2]] = l[3:].strip()
                i += 1
                continue
            if len(cells) == len(days):
                break
            text = l
            while text.lower() not in CATEGORIES or (
                text.lower() == "not visible with a" or text.lower() == "visible in perfect" or
                text.lower() in ("may need optical", "will need optical")):
                i += 1
                text += " " + body[i]
                if len(text) > 60:
                    raise ValueError(f"unrecognised category near {text!r}")
            cells.append({"cat": text})
            i += 1
        name = " ".join(name_parts)
        for d, c in zip(days, cells):
            rec = [f"{year:04d}-{month:02d}-{d:02d}", name, zone, f"{offset_h:+g}", c["cat"],
                   CATEGORIES[c["cat"].lower()], c.get("SS", ""), c.get("MS", "")]
            for key in ("SS", "MS"):
                t = c.get(key, "")
                if re.match(r"^\d\d:\d\d$", t):
                    loc = datetime(year, month, d, int(t[:2]), int(t[3:]))
                    rec.append((loc - timedelta(hours=offset_h)).strftime("%Y-%m-%d %H:%M"))
                else:
                    rec.append("")
            rows.append(rec)
    return rows


def main(argv):
    if len(argv) % 3:
        sys.exit(__doc__)
    sys.stdout.reconfigure(newline="\n")  # LF output on Windows too
    w = csv.writer(sys.stdout, lineterminator="\n")
    w.writerow(["local_date", "location", "zone", "utc_offset_h", "hmnao_text", "yallop_category",
                "sunset_local", "moonset_local", "sunset_ut", "moonset_ut"])
    for k in range(0, len(argv), 3):
        w.writerows(parse(int(argv[k]), int(argv[k + 1]), argv[k + 2]))


if __name__ == "__main__":
    main(sys.argv[1:])
