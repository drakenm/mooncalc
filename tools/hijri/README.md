# Hijri data tools

`gen_umalqura.py` regenerates the Umm al-Qura tables embedded in `mooncalc-hijri`:

- `crates/mooncalc-hijri/src/umalqura/data.rs`: the `OFFICIAL` (1343–1500 AH) and `ICU` (1300–1600 AH) tables.
- `crates/mooncalc-hijri/tests/data/umalqura_reference.rs`: 1,892 reference dates used by the integration test.

It needs only Python 3 and the standard library. It parses the downloaded sources as text and never imports or executes them.

## Sources

Download each file into an empty scratch directory (not the repository). The commits are pinned, so the output is reproducible.

| File | URL | SHA-256 | Licence |
| --- | --- | --- | --- |
| `ummalqura.py` | `https://raw.githubusercontent.com/dralshehri/hijridate/310b9846cefc3856eeec4394831c723ab5357d17/src/hijridate/ummalqura.py` | `cf5b3758027a72a4ab452b25f996aabd0af99de884ddb72f0f35c2a9c4b94eb5` | MIT |
| `ummalqura_newspaper_issues.json` | same commit, `tests/integration/fixtures/` | `7b7460aaac8a818a49f56ed5d96d1be37ab1f40a79168b5c0a1dc975dd60959f` | MIT |
| `kfupm_comparison_calendar.json` | same commit, `tests/integration/fixtures/` | `416b353f5ed29e2f9b984008d176914cd4817def7c3a2b2b5bc1c5d34ac84c73` | MIT |
| `ummalqura_calendar_website.json` | same commit, `tests/integration/fixtures/` | `bfe6e587f0d1e2f2b3d7daa9b4d80f4eae95aa4c75717760c7d729a432c2e436` | MIT |
| `islamcal.cpp` | `https://raw.githubusercontent.com/unicode-org/icu/59af52bb6f967bc5061bc3f6a91063de88e4aa78/icu4c/source/i18n/islamcal.cpp` | `a665b4eed397fc890786a27d27e80c754f71620101d79bc6a2b1bfa7d00bb6cb` | Unicode-3.0 (and the earlier ICU licence) |
| `UmAlQuraCalendar.cs` (cross-check only, nothing embedded) | `https://raw.githubusercontent.com/dotnet/runtime/60685192899cbd2f47534e17dc2861bfd7cb77b9/src/libraries/System.Private.CoreLib/src/System/Globalization/UmAlQuraCalendar.cs` | `12eb6a025be12aa9cb6175336c0c29db735653471ca32341d96bbb7a1a9e165d` | MIT |

The licence texts and attributions are in [`crates/mooncalc-hijri/THIRD-PARTY-NOTICES.md`](../../crates/mooncalc-hijri/THIRD-PARTY-NOTICES.md).

## Running

From the repository root, with `$D` the scratch directory:

```sh
python -I tools/hijri/gen_umalqura.py \
    --hijridate "$D/ummalqura.py" \
    --icu "$D/islamcal.cpp" \
    --dotnet "$D/UmAlQuraCalendar.cs" \
    --fixtures "$D/ummalqura_newspaper_issues.json" \
               "$D/kfupm_comparison_calendar.json" \
               "$D/ummalqura_calendar_website.json" \
    --out-data crates/mooncalc-hijri/src/umalqura/data.rs \
    --out-tests crates/mooncalc-hijri/tests/data/umalqura_reference.rs
cargo fmt --all
```

The script checks that consecutive years in each table abut, and that every reference date converts correctly with the official table. It exits non-zero on any mismatch. It also prints a comparison report, which it copies into the header of `data.rs`. With the pinned sources the report reads:

```text
official (hijridate): 1343-1500 AH
icu:                  1300-1600 AH
icu differs from official in years: 1343-1419, 1451-1500
.NET (1318-1500) differs from official in years: 1343-1394, 1400-1401
fixture ummalqura_newspaper_issues.json: 380 rows, 0 mismatches
fixture kfupm_comparison_calendar.json: 672 rows, 0 mismatches
fixture ummalqura_calendar_website.json: 840 rows, 0 mismatches
```

## Why two Umm al-Qura tables

- **Official** (`umm-al-qura`): the month starts as Saudi Arabia published them. hijridate checked them against archived *Umm al-Qura* newspaper issues (1343–1355 AH), the KFUPM comparison calendar (1356–1411 AH), the official comparative calendar books (1412–1450 AH) and the KACST website (1451–1500 AH). The years 1451–1500 are projections, which KACST may revise. Before 1365 AH the published calendar contains a few 28- and 31-day months. .NET's independent table agrees for 1395–1399 and 1402–1500 AH.
- **ICU** (`umm-al-qura-icu`): CLDR's `islamic-umalqura`, the calendar behind `Intl.DateTimeFormat` in browsers. It covers a wider range, but outside 1420–1450 AH it is an algorithmic reconstruction that differs from the published calendar. It is kept for interoperability with web frontends and for years outside the official table.
