#!/usr/bin/env python3
"""Generate the Umm al-Qura tables embedded in `mooncalc-hijri`.

Usage (run with `python -I`, passing every downloaded file as an argument):

    python -I tools/hijri/gen_umalqura.py \
        --hijridate  <dir>/ummalqura.py \
        --icu        <dir>/islamcal.cpp \
        --dotnet     <dir>/UmAlQuraCalendar.cs \
        --fixtures   <dir>/ummalqura_newspaper_issues.json \
                     <dir>/kfupm_comparison_calendar.json \
                     <dir>/ummalqura_calendar_website.json \
        --out-data   crates/mooncalc-hijri/src/umalqura/data.rs \
        --out-tests  crates/mooncalc-hijri/tests/data/umalqura_reference.rs

See `tools/hijri/README.md` for the pinned source URLs and licences.

The script only reads its inputs; it never imports or executes them.
"""

import argparse
import json
import re
import sys

# Internally each year is (JDN of 1 Muharram, [12 month lengths]).


def g2jdn(y, m, d):
    """Proleptic Gregorian date to Julian Day Number (integer, at noon)."""
    a = (14 - m) // 12
    yy = y + 4800 - a
    mm = m + 12 * a - 3
    return d + (153 * mm + 2) // 5 + 365 * yy + yy // 4 - yy // 100 + yy // 400 - 32045


def parse_hijridate(text):
    """hijridate's MONTH_STARTS: Reduced Julian Days (JDN - 2400000)."""
    first = int(re.search(r"HIJRI_RANGE[^=]*=\s*\(\((\d+),\s*1,\s*1\)", text).group(1))
    body = text[text.index("MONTH_STARTS"):]
    body = body[body.index("(") + 1:body.index(")")]
    starts = [int(x) + 2400000 for x in re.findall(r"\d+", body)]
    if (len(starts) - 1) % 12:
        sys.exit("hijridate: month count is not whole years plus a sentinel")
    years = []
    for i in range((len(starts) - 1) // 12):
        ms = starts[12 * i:12 * i + 13]
        lengths = [ms[k + 1] - ms[k] for k in range(12)]
        if any(length not in (28, 29, 30, 31) for length in lengths):
            sys.exit(f"hijridate: implausible month length in {first + i}: {lengths}")
        years.append((ms[0], lengths))
    return first, years


def parse_icu(text):
    """ICU islamcal.cpp: UMALQURA_MONTHLENGTH (bit 11 = month 1) plus the
    least-squares year-start estimate and its correction table."""
    first = int(re.search(r"UMALQURA_YEAR_START\s*=\s*(\d+)", text).group(1))
    last = int(re.search(r"UMALQURA_YEAR_END\s*=\s*(\d+)", text).group(1))
    civil_epoch = int(re.search(r"CIVIL_EPOC\s*=\s*(\d+)", text).group(1))
    body = text[text.index("UMALQURA_MONTHLENGTH[] = {"):]
    body = body[:body.index("\n};")]
    raw = []
    for line in body.splitlines():
        if line.strip().startswith("//"):
            continue
        raw += [int(h, 16) for h in re.findall(r"0x[0-9A-Fa-f]{4}", line)]
    fb = text[text.index("umAlQuraYrStartEstimateFix[] = {"):]
    fb = fb[fb.index("{") + 1:fb.index("};")]
    fix = []
    for line in fb.splitlines():
        fix += [int(x) for x in re.findall(r"-?\d+", line.split("//")[0])]
    n = last - first + 1
    if len(raw) != n or len(fix) != n:
        sys.exit(f"icu: expected {n} years, got {len(raw)} masks and {len(fix)} fixes")
    years = []
    for i in range(n):
        start = civil_epoch + int(354.36720 * i + 460322.05 + 0.5) + fix[i]
        lengths = [29 + ((raw[i] >> (11 - k)) & 1) for k in range(12)]
        years.append((start, lengths))
    return first, years


def parse_dotnet(text):
    """.NET UmAlQuraCalendar: masks with bit 0 = month 1, Gregorian starts."""
    body = text[text.index("ReadOnlySpan<short> rawData"):]
    rows = re.findall(r"(\d{4})\*/0x([0-9A-Fa-f]{4}),\s*(\d{4}),\s*(\d+),\s*(\d+),", body)
    return {int(y): (g2jdn(int(gy), int(gm), int(gd)),
                     [29 + ((int(h, 16) >> k) & 1) for k in range(12)])
            for y, h, gy, gm, gd in rows}


def year_length(lengths):
    return sum(lengths)


def check_continuity(name, years):
    for i in range(len(years) - 1):
        if years[i][0] + year_length(years[i][1]) != years[i + 1][0]:
            sys.exit(f"{name}: year index {i} does not abut the next year")


def month_starts(first, years):
    out = {}
    for i, (start, lengths) in enumerate(years):
        s = start
        for k in range(12):
            out[(first + i, k + 1)] = s
            s += lengths[k]
    return out


def to_hijri(first, years, jdn):
    for i, (start, lengths) in enumerate(years):
        if start <= jdn < start + year_length(lengths):
            d = jdn - start
            for k, length in enumerate(lengths):
                if d < length:
                    return (first + i, k + 1, d + 1)
                d -= length
    return None


def diff_years(a, b):
    return sorted({ym[0] for ym in a if ym in b and a[ym] != b[ym]})


def ranges(ys):
    out = []
    for y in ys:
        if out and out[-1][1] == y - 1:
            out[-1][1] = y
        else:
            out.append([y, y])
    return ", ".join(f"{a}" if a == b else f"{a}-{b}" for a, b in out) or "none"


def emit_table(name, first, years):
    lines = [f"pub(super) const {name}_FIRST_YEAR: i32 = {first};",
             f"pub(super) static {name}: [YearEntry; {len(years)}] = ["]
    for i, (start, lengths) in enumerate(years):
        ls = ", ".join(str(x) for x in lengths)
        lines.append(f"    YearEntry::new({start}, [{ls}]), // {first + i}")
    lines.append("];")
    return "\n".join(lines)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--hijridate", required=True)
    ap.add_argument("--icu", required=True)
    ap.add_argument("--dotnet")
    ap.add_argument("--fixtures", nargs="*", default=[])
    ap.add_argument("--out-data", required=True)
    ap.add_argument("--out-tests")
    a = ap.parse_args()

    read = lambda p: open(p, encoding="utf-8").read()
    h_first, h_years = parse_hijridate(read(a.hijridate))
    i_first, i_years = parse_icu(read(a.icu))
    check_continuity("hijridate", h_years)
    check_continuity("icu", i_years)
    h_ms = month_starts(h_first, h_years)
    i_ms = month_starts(i_first, i_years)

    report = [f"official (hijridate): {h_first}-{h_first + len(h_years) - 1} AH",
              f"icu:                  {i_first}-{i_first + len(i_years) - 1} AH",
              f"icu differs from official in years: {ranges(diff_years(h_ms, i_ms))}"]
    if a.dotnet:
        net = parse_dotnet(read(a.dotnet))
        n_ms = month_starts(min(net), [net[y] for y in sorted(net)])
        report.append(f".NET ({min(net)}-{max(net)}) differs from official in years: "
                      f"{ranges(diff_years(h_ms, n_ms))}")

    refs = []
    for path in a.fixtures:
        bad = 0
        rows = json.loads(read(path))
        for row in rows:
            (hy, hm, hd), (gy, gm, gd) = row[0]
            jdn = g2jdn(gy, gm, gd)
            got = to_hijri(h_first, h_years, jdn)
            if got != (hy, hm, hd):
                bad += 1
                print(f"MISMATCH {path}: {row[0]} -> {got}", file=sys.stderr)
            refs.append((hy, hm, hd, gy, gm, gd))
        report.append(f"fixture {path.replace(chr(92), '/').split('/')[-1]}: "
                      f"{len(rows)} rows, {bad} mismatches")
        if bad:
            sys.exit("fixture mismatch")
    for line in report:
        print(line)

    header = (
        "// @generated by tools/hijri/gen_umalqura.py. Do not edit by hand.\n"
        "//\n"
        "// OFFICIAL: Umm al-Qura month starts as officially published, from the\n"
        "// hijridate project (MIT licence, Copyright (c) 2018-2025 Mohammed\n"
        "// Alshehri); see crates/mooncalc-hijri/THIRD-PARTY-NOTICES.md.\n"
        "// ICU: the Umm al-Qura table of ICU4C islamcal.cpp (Unicode License V3,\n"
        "// Copyright (c) 1991-2026 Unicode, Inc.; see the same notices file).\n"
        "//\n"
        "// Each entry is (JDN of 1 Muharram, lengths of months 1-12).\n"
        "//\n"
        + "".join(f"// {line}\n" for line in report)
    )
    data = (header + "\nuse super::YearEntry;\n\n"
            + emit_table("OFFICIAL", h_first, h_years) + "\n\n"
            + emit_table("ICU", i_first, i_years) + "\n")
    with open(a.out_data, "w", encoding="utf-8", newline="\n") as f:
        f.write(data)

    if a.out_tests:
        refs = sorted(set(refs))
        body = ["// @generated by tools/hijri/gen_umalqura.py. Do not edit by hand.",
                "//",
                "// Umm al-Qura reference dates from the hijridate test fixtures",
                "// (archived Umm al-Qura newspaper issues, the KFUPM comparison calendar",
                "// and the official Umm al-Qura website). MIT licence, Copyright (c)",
                "// 2018-2025 Mohammed Alshehri; see THIRD-PARTY-NOTICES.md.",
                "//",
                "// (hijri year, month, day, Gregorian year, month, day)",
                f"pub static UMALQURA_REFERENCE: [(i32, u8, u8, i32, u8, u8); {len(refs)}] = ["]
        body += [f"    ({hy}, {hm}, {hd}, {gy}, {gm}, {gd})," for hy, hm, hd, gy, gm, gd in refs]
        body.append("];")
        with open(a.out_tests, "w", encoding="utf-8", newline="\n") as f:
            f.write("\n".join(body) + "\n")


if __name__ == "__main__":
    main()
