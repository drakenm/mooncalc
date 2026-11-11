# Validation

Reference cases live in `tests/fixtures/`, one directory per topic. Each directory has a `README.md` giving the source, units, time scale and frame of its files. This page is the full reference: where every dataset comes from, its terms of use, how it was produced, and the tolerance a test should use against it.

Everything here was retrieved on **2026-11-11**. Fixtures are plain CSV (UTF-8, LF, one header row, no comment lines) or JSON. Empty CSV fields mean "not given by the source".

## Regenerating

The scripts in `tools/fixtures/` use only the Python standard library. Run them from the repository root with `python -I`.

| Script | Produces |
| --- | --- |
| `fetch_fixtures.py <target>... \| all` | Every API- or web-fetched fixture: `horizons-geo`, `horizons-topo`, `usno-rstt`, `usno-phases`, `deltat`, `eclipses`, `hijri`. Requests are sequential, 1.5 s apart, with User-Agent `mooncalc-validation-fixtures` and no other identifying data. |
| `extract_yallop_table4.py` | `crescent/yallop1997_table4.csv` from `pdftotext -raw` output of the Yallop PDF, with built-in consistency checks. |
| `check_odeh2004_table6.py` | Consistency checks for the hand-transcribed Odeh table. |
| `pdf_ccitt_to_tiff.py` | Full-resolution page images of the Odeh scan, for re-checking the transcription. |
| `extract_hmnao_summary.py` | `predictions/hmnao_city_visibility.csv` from `pdftotext -raw` output of HMNAO monthly summaries. |
| `espenak_meeus_deltat.py` | `deltat/espenak_meeus_polynomial_checks.csv`. |

Re-fetching changes some values slightly: Horizons applies the latest Earth orientation parameters, and USNO and van Gent update their pages. Review diffs rather than accepting them blindly.

## Crescent sightings and criteria (`crescent/`)

### `yallop1997_table4.csv`: Yallop (1997) Table 4, 295 rows

- **Source:** B.D. Yallop, *A Method for Predicting the First Sighting of the New Crescent Moon*, NAO Technical Note No. 69 (1997; PDF revision dated 2004-09-22). PDF: <https://www.astronomycenter.net/pdf/yallop_1997.pdf>. HMNAO's own copy (`astro.ukho.gov.uk/download/NAOTN69.pdf`) was unreachable at retrieval time.
- **Terms:** Crown copyright (HM Nautical Almanac Office). The fixture reproduces factual tabular data with citation, for testing.
- **Content:** the full Table 4: all 295 Schaefer–Doggett first-sighting records as recomputed by Yallop at his best time. There is one row per record, in the PDF's order (decreasing q).
- **Columns:** `no` (Schaefer list number); `year,month,day` (local civil date of the observation); `me` (M morning, E evening); `jd_newmoon` (JD of geocentric conjunction, the table's "JD − 2 400 000" column plus 2 400 000, time scale not stated); `lat_deg`, `lon_deg` (east positive); `arcl_deg`, `arcv_deg`, `daz_deg` (degrees, at best time; DAZ = Sun azimuth − Moon azimuth); `age_h` (hours, negative for morning); `lag_min` (negative for morning); `parallax_arcmin` (lunar horizontal parallax); `w_topo_arcmin` (topocentric width W′); `q`; `vis_observed` (Schaefer's code, e.g. `V`, `I(V)`, `V(F)`); `vis_predicted` (Yallop's code from q); `q_category` (A–F, from the table's section headings).
- **Frame and conventions:** ARCL and ARCV are geocentric and airless, as Yallop defines them in §2 ("ignoring the effects of refraction"). W′ is topocentric (eqs 3.8–3.10). Best time is Tb = Ts + (4/9)·Lag (eq 4.1).
- **Extraction checks:** 295 distinct record numbers. Category counts are A 166, B 68, C 26, D 14, E 4 and F 17; these match the section headings. The §6 prose says "17 cases" for D, but the heading and the rows give 14. Every q reproduces from ARCV and W′ through eq (6.1) within the rounding of the printed values. Every ARCL below 22° reproduces from ARCV and DAZ through eq (2.1) within 0.15°. Every observation date lies within the expected window around `jd_newmoon`.
- **Use:** (1) Criterion unit tests. Feed the tabulated `arcv_deg` and `w_topo_arcmin` into the Yallop q function and compare with `q`. (2) Classification. q must map to `q_category`. (3) End-to-end. Compute ARCL, ARCV, DAZ, W′, lag and age from the ephemeris at the site and date, then compare.
- **Tolerances:** (1) |Δq| ≤ (0.005 + |dq/dW′|·½ulp(W′)), roughly 0.006–0.012. Simpler: ≤ 0.012, or ≤ 0.035 when W′ ≥ 1′. (2) exact except within 0.012 of a boundary. (3) ARCL, ARCV, DAZ ≤ 0.2°, lag ≤ 2 min, age ≤ 0.2 h, q ≤ 0.02. Yallop's best time and his ephemeris (1997) are not exactly reproducible, and the tabulated values are rounded to 0.1°.

### `odeh2004_table6.csv`: Odeh (2004) Table VI, 578 of 737 rows

- **Source:** M.Sh. Odeh, "New criterion for lunar crescent visibility", *Experimental Astronomy* 18:39–64 (2004; Springer 2006), doi:10.1007/s10686-005-9002-5. PDF copy: <http://bruneiastronomy.org/web/wp-content/uploads/2015/07/New_Criterion_Crescent.pdf>.
- **Terms:** journal article © Springer. The fixture is a transcription of the factual observation table, with citation, for testing. *Maintainers: confirm this is acceptable before publishing the repository.*
- **How it was made:** the table pages (journal pp. 44–60) are scanned images with no text layer. They were read visually from full-resolution renderings (`pdf_ccitt_to_tiff.py`) and typed into CSV. `check_odeh2004_table6.py` then recomputes V from ARCV and W (eq 2) and ARCL from ARCV and DAZ. It also derives the local solar date from JD and longitude, and checks the age sign and the ordering of V. All rows pass except one printed inconsistency (record 737, below). Independent spot checks against the page images found no discrepancies: 12 consecutive rows, and three rows whose coordinates differ from Yallop's for the same observation. Values are copied as printed: leading zeros, observer spellings, blank observer and N/B/T fields.
- **Coverage gap:** the PDF's table holds **578 rows** (V from −5.96 to 8.18) and is complete for every record with V ≤ 8.18. The 159 missing record numbers are the easiest naked-eye sightings, with V above 8.18 (for example, Schaefer records with Yallop q ≳ 0.45). The table in this PDF stops after six rows on p. 60. Whether the printed journal includes them was not verified.
- **Columns:** `no` (Odeh record number, chronological); `ref` (A Schaefer, B Stamm, C SAAO, D Mirsaeed, F Mehrani, I ICOP); `me`; `date` (local date dd-mm-yyyy, as printed); `observer`; `lon_deg` (east positive), `lat_deg`, `elev_m`; `naked`, `binoc`, `tele` (I invisible, V visible, blank not tried); `jd_best` (JD at best time; Accurate Times convention, assume UT); `age_h` (topocentric age, negative for morning); `lag_min`; `arcv_deg`, `daz_deg`, `arcl_deg` (topocentric, airless, at best time); `w_arcsec` (topocentric width in **arcseconds**; divide by 60 for the arcminutes eq 2 expects); `v`; `group` (the printed group heading); `page` (journal page).
- **Groups:** groups D, C, B and A are the criterion zones by V (thresholds −0.96, 2, 5.65). Group C begins at record 737 (V = −0.96), mid p. 45. Group B begins on p. 49 and group A on p. 56. The single row out of step is record 433: V = 1.99, printed under group B.
- **Known printed inconsistencies:** record 737 gives date 02-11-2006, but its JD is 2005-11-02. Record 126 has longitude −28.2 at latitude −25.8 (Pretoria is +28.2); Yallop's Table 4 has the same sign for this record, so the error is in the original Schaefer list. Records 138 and 142 have each other's coordinates compared with Yallop's records for 1976-12-21 and 1977-02-18. Record 212's longitude is −81.0, where Yallop has −84.0. Observer spellings vary: McPartian/McPartlan, Degroupet/DeCroupet, Fcfferman.
- **Use:** (1) Criterion unit tests. Compute V from `arcv_deg` and `w_arcsec/60` and compare with `v`. The zone must match `group`, except near thresholds. (2) End-to-end topocentric ARCV, DAZ, ARCL and W at `jd_best` for the site, including `elev_m`.
- **Tolerances:** (1) |ΔV| ≤ 0.12 (W printed to 1″, ARCV to 0.1°). (2) ARCV, DAZ, ARCL ≤ 0.15°, W ≤ 2″, lag ≤ 2 min, age ≤ 0.2 h.

### `criteria_published_constants.json`

Constants transcribed from the two papers: Yallop's q polynomial, the A–F thresholds of Table 5, the semi-diameter and width formulas, best time, and the Maunder, Indian and Bruin tables and fits (eqs 3.1–3.7). From Odeh: the V polynomial, the A–D thresholds, Table V (ARCV1–3 against W), and the SAAO Table IV as reproduced there. Constants must match exactly.

## Ephemeris (`ephemeris/`)

### `horizons_geocentric.csv`: JPL Horizons geocentric Sun and Moon, 88 rows

- **Source:** JPL Horizons API <https://ssd.jpl.nasa.gov/api/horizons.api>, `EPHEM_TYPE=OBSERVER`, `CENTER='500@399'`, `QUANTITIES='2,10,13,20,23,30,31'`, `EXTRA_PREC=YES`, `ANG_FORMAT=DEG`, `TIME_TYPE=TT`, discrete `TLIST`. Ephemeris DE441 (Horizons' current solution); EOP file `eop.261110.p270205`.
- **Terms:** JPL/NASA data. No usage restrictions known; credit "JPL Horizons".
- **Epochs:** 44 TT epochs for each of the Sun (10) and Moon (301): 1600, 1650, 1700, 1750, 1800, 1850 and 1875; every 8 years from 1900 to 1996; every 4 years from 1996 to 2028; every 8 years from 2036 to 2100; then 2125, 2150, 2175 and 2200. Day of year and time are varied. Also J2000.0 (JD 2451545.0) and the epochs of Meeus (1998) examples 25.a (JD 2448908.5) and 47.a (JD 2448724.5).
- **Columns:** `body`; `jd_tt` (the input epoch, TT); `calendar_tt` (Gregorian); `tdb_minus_ut_s` (Horizons' TDB−UT: before 1962 ≈ ΔT = TT−UT1; **from 1962 it is TDB−UTC**, i.e. 32.184 s + leap seconds, and the last value is held constant into the future, so it is not ΔT); `ra_app_deg`, `dec_app_deg` (airless apparent RA/Dec, true equator and equinox of date, IAU76/80 with EOP corrections; Horizons notes the RA origin is offset −53 mas from the IAU 2006/2000A equinox); `ecl_lon_app_deg`, `ecl_lat_app_deg` (apparent ecliptic longitude and latitude of date, IAU76/80); `delta_au`, `delta_km` (light-time-aberrated apparent range, 1 au = 149 597 870.700 km); `ang_diam_arcsec`; `illum_pct`; `elong_sot_deg` (Sun–observer–target elongation, i.e. geocentric ARCL for the Moon).
- **Corrections included:** light time, gravitational deflection, aberration, precession and nutation. No refraction (geocentric).
- **Tolerances:** depend on the backend and the epoch.
  - **Meeus/ELP backend, 1900–2100:** Sun λ, RA and Dec ≤ 0.003° (10″) with full VSOP87D, or ≤ 0.01° with the ch. 25 low-precision method. Moon λ, RA and Dec ≤ 0.005° (18″), β ≤ 0.003°, distance ≤ 15 km.
  - **Meeus/ELP backend, outside 1900–2100:** double those limits.
  - **DE440 backend:** RA, Dec and λ ≤ 0.2″ for 1962–2026, where EOP nutation corrections apply, and ≤ 0.5″ otherwise. Moon distance ≤ 0.1 km. Sun distance ≤ 10 km.
  - **Elongation:** compare `elong_sot_deg` with the computed ARCL at the same tolerance as the positions.

### `horizons_topocentric.csv`: JPL Horizons topocentric alt/az, 60 rows

- **Source:** as above, with `CENTER='coord@399'`, `COORD_TYPE=GEODETIC`, `SITE_COORD='lon,lat,h_km'`, `QUANTITIES='2,4,13,20'` and `TIME_TYPE=UT`. Each epoch is queried twice, with `APPARENT=AIRLESS` and with `APPARENT=REFRACTED`.
- **Sites (test inputs, approximate):** Mecca (39.8262 E, 21.4225 N, 277 m), Greenwich (−0.0015, 51.4769, 46 m), Cape Town (18.4241, −33.9249, 10 m), Kuala Lumpur (101.6869, 3.1390, 60 m), Los Angeles (−118.2437, 34.0522, 90 m). Each site has six UT epochs: 18:30 local mean time on 2024-03-11, 2024-04-09 and 2025-03-30 (evening crescent dates), plus 2000-01-01 12:00, 2015-06-15 06:00 and 2030-09-01 21:30 UT.
- **Time scale:** Horizons "UT" is **UTC from 1962 on** (UT1 before 1962). `jd_ut` is authoritative; `calendar_ut` carries the 6-decimal JD rounding (±0.04 s).
- **Columns:** `site`, `lon_deg`, `lat_deg`, `height_m`, `body`, `jd_ut`, `calendar_ut`; `ra_app_topo_deg`, `dec_app_topo_deg` (airless topocentric apparent of date); `az_airless_deg`, `alt_airless_deg`; `az_refracted_deg`, `alt_refracted_deg` (Horizons' "approximate atmospheric yellow-light refraction"; it is also applied below the horizon); `delta_topo_km`; `ang_diam_arcsec`. Azimuth is measured from north through east.
- **Tolerances:** airless alt/az ≤ 0.01°. UT1−UTC (< 0.9 s) shifts the hour angle by up to 13″, which is why the limit is not tighter for DE440. Topocentric RA/Dec, as in the geocentric case, plus 1″. Refracted altitude: use only rows with `alt_airless_deg` > 0; ≤ 0.02° above 10°, ≤ 0.1° from 0° to 10°. The refraction model and its default pressure and temperature differ from Bennett/Saemundsson.

## Rise, set and transit (`riseset/usno_rstt.csv`), 558 rows

- **Source:** USNO Astronomical Applications API, `https://aa.usno.navy.mil/api/rstt/oneday?date=YYYY-MM-DD&coords=lat,lon&tz=0`. No `height`, so sea level; no ID parameter.
- **Terms:** U.S. Government work (U.S. Naval Observatory), public domain in the U.S.
- **Sites and dates:** the five topocentric sites plus Tromsø (18.9560 E, 69.6496 N). Ten dates each: 2024-01-15, -03-20, -06-21, -09-22 and -12-21; 2025-02-28, -05-10, -08-05 and -11-01; 2026-01-20. Tromsø has four more (2024-05-20, 2024-07-25, 2024-11-25, 2025-01-17) for the midnight sun, polar night and transitions.
- **Columns:** `site`, `lon_deg`, `lat_deg`, `date_ut` (with `tz=0`, events falling in that UT calendar day); `body`; `event`; `time_ut` (HH:MM, UT1 per the API docs, rounded to the minute); `usno_phen` (USNO's wording). `event` is one of `rise`, `set`, `upper_transit`, `lower_transit`, `civil_twilight_begin` or `civil_twilight_end`. It can also be a state for the whole day: `continuously_above_horizon`, `continuously_below_horizon` or `continuously_above_twilight_limit`. Each site-date also has a `fracillum` row for the Moon, with the illuminated percentage in `time_ut` and the phase name in `usno_phen`.
- **Definitions** (USNO, <https://aa.usno.navy.mil/faq/RST_defs>): sunrise and sunset occur when the upper limb is on the horizon, with 34′ refraction and 16′ semi-diameter, so the Sun's centre is at zenith distance 90°50′. Moonrise and moonset are defined similarly, using topocentric parallax and the Moon's semi-diameter. Civil twilight is the Sun's centre at −6°. The observer is on a level horizon.
- **Tolerances:** Sun ±1 min and Moon ±2 min at latitudes below 60°. At Tromsø, ±5 min for the Sun near the polar-day and polar-night transitions. Continuous-state flags must match exactly, except within one day of a transition.

## Moon phases (`phases/usno_moon_phases.csv`), 1683 rows

- **Source:** USNO API `https://aa.usno.navy.mil/api/moon/phases/year?year=Y` for 2000–2030, plus 1700, 1800 and 1900. Terms as for USNO above.
- **Columns:** `date_ut`, `time_ut` (HH:MM, Universal Time, rounded to the minute), `phase` (`New Moon`, `First Quarter`, `Full Moon`, `Last Quarter`). These are the geocentric instants at which the apparent ecliptic longitude difference Moon − Sun is 0°, 90°, 180° or 270°.
- **Tolerance:** ±1 min, given the rounding. For 1700, 1800 and 1900, USNO's ΔT is not stated; any difference in ΔT shifts the UT instant one-for-one, so allow ±1 min plus that difference.

## ΔT (`deltat/`)

| File | Rows | Source | Content |
| --- | --- | --- | --- |
| `usno_observed_deltat.csv` | 685 | USNO/IERS Rapid Service: `https://maia.usno.navy.mil/ser7/historic_deltat.data` (half-yearly 1657.0–1972.5) and `.../deltat.data` (values for 1 January of each year, 1974–2026) | `year` (decimal), `delta_t_s` (TT−UT1), `error_s` (historic file only), `source` |
| `nasa_deltat_historical.csv` | 28 | Espenak, "Historical Values of Delta T", <https://eclipse.gsfc.nasa.gov/SEhelp/deltat2004.html> Table 1 (Morrison & Stephenson 2004) | `year` (−500 to 1950), `delta_t_s`, `std_error_s` (as printed, e.g. `<1`) |
| `nasa_deltat_recent.csv` | 11 | same page, Table 2 (Astronomical Almanac 2006, p. K9) | `year` (1955–2005), `delta_t_s` |
| `espenak_meeus_polynomial_checks.csv` | 42 | **Computed** by `espenak_meeus_deltat.py` from the expressions at <https://eclipse.gsfc.nasa.gov/SEhelp/deltatpoly2004.html> | `year` (decimal y), `delta_t_s`, `lunar_correction_c_s` |

- **Terms:** USNO is public domain. NASA data may be reproduced with the acknowledgment the site requests: "Eclipse Predictions by Fred Espenak (NASA's GSFC)".
- **Tolerances:**
  - **Polynomial checks:** ≤ 0.001 s. They pin an implementation of the same polynomials, branch boundaries included: each piece applies for `start ≤ y < end`.
  - **Observed values:** the Espenak–Meeus polynomial matches USNO to ≤ 1 s for 1900–2005. After 2005 it is an extrapolation that drifts away: 71.6 s against 69.4 s observed in 2020, and about 75.1 s against 69.1 s in 2026. An implementation that only uses the polynomial needs ≤ 7 s for 2005–2026. One that interpolates the observed table should match it to ≤ 0.1 s. For 1657–1900, allow ≤ 3 s or the stated error, whichever is larger.
  - **NASA Table 1:** within the standard error, or 4 s for −500 to +500, where the page says the polynomial reproduces Table 1 to 4 s.

## Hijri (`hijri/`)

| File | Rows | Content |
| --- | --- | --- |
| `ummalqura_principal_days.csv` | 31 | Umm al-Qura Gregorian dates of 1 Muḥarram, 12 Rabīʿ I, 1 Ramaḍān, 1 Shawwāl and 10 Dhu ʾl-Ḥijja for 1420–1450 AH |
| `saudi_announced_vs_ummalqura.csv` | 135 | 1422–1448 AH. For 1 Muḥarram, 1 Ramaḍān, 1 Shawwāl, 1 Dhu ʾl-Ḥijja and 10 Dhu ʾl-Ḥijja: `ummalqura_gregorian` (computed) and `announced_gregorian` (the date the Saudi High Judiciary Council announced after reported sightings; blank when none is tabulated) |

- **Source:** R.H. van Gent, *The Umm al-Qura Calendar of Saudi Arabia*, Utrecht University: <https://webspace.science.uu.nl/~gent0113/islam/ummalqura_principal.htm> and `.../ummalqura_adjust.htm`. The announcements are as compiled there from Fatwa-Online.
- **Terms:** © R.H. van Gent. Factual dates, cited for testing.
- **Notes:** 22 rows differ between computed and announced dates. They match the 17 month-start cases van Gent lists, where the crescent was reported a day after or a day before the computed date, plus the five 10 Dhu ʾl-Ḥijja rows that follow from them. Announced dates are observational decisions, not predictions; a visibility calendar should not be expected to reproduce the early ones (van Gent notes moonset before sunset in several cases). Each date is the Gregorian civil day on which the Hijri day begins at the preceding sunset.
- **Tolerances:** Umm al-Qura computed dates must match exactly. For announced dates, use them as observational cross-checks: report agreement rates rather than asserting.

## Eclipses (`eclipses/`)

- **Source:** NASA Five Millennium Catalogs (Espenak & Meeus): <https://eclipse.gsfc.nasa.gov/SEcat5/SE1901-2000.html>, `SE2001-2100.html`, `../LEcat5/LE1901-2000.html` and `LE2001-2100.html`. The predictions use VSOP87 for the Sun and ELP-2000/82 for the Moon. ΔT is from Morrison & Stephenson (2004) before the 1950s and observed from 1955.
- **Terms:** "Permission is freely granted to reproduce this data when accompanied by an acknowledgment: 'Eclipse Predictions by Fred Espenak (NASA's GSFC)'."
- **`nasa_solar_eclipses.csv` (13 rows):** 1919-05-29, 1973-06-30, 1999-08-11, 2001-06-21, 2017-08-21, 2022-10-25 (P), 2023-04-20 (H), 2023-10-14 (A), 2024-04-08, 2024-10-02 (A), 2025-03-29 (P), 2026-08-12 and 2027-08-02. Columns: `cat_no`, `date_td`, `greatest_td` (TD ≈ TT), `delta_t_s`, `lunation`, `saros`, `type` (P/A/T/H, with catalog suffixes), `qle`, `gamma`, `magnitude`, `lat`, `lon` (point of greatest eclipse, e.g. `25N`, `104W`), `sun_alt_deg`, `path_width_km` and `central_duration` (blank for partial eclipses).
- **`nasa_lunar_eclipses.csv` (11 rows):** 1982-07-06, 2000-07-16, 2018-07-27, 2019-01-21, 2022-11-08, 2023-05-05 (N), 2023-10-28 (P), 2024-03-25 (N), 2025-03-14, 2025-09-07 and 2026-03-03. Columns: `cat_no`, `date_td`, `greatest_td`, `delta_t_s`, `lunation`, `saros`, `type` (N/P/T with suffixes), `qse`, `gamma`, `pen_magnitude`, `um_magnitude`, `dur_pen_min`, `dur_par_min`, `dur_total_min` (`-` where none), `zenith_lat` and `zenith_lon`.
- **Tolerances:** greatest eclipse ≤ 30 s (TD) with the Meeus backend, ≤ 5 s with DE440. Gamma ≤ 0.001, or ≤ 0.005 for the simplified series of Meeus ch. 54. Magnitudes ≤ 0.005. The type must match.

## Published visibility predictions (`predictions/hmnao_city_visibility.csv`), 192 rows

- **Source:** HM Nautical Almanac Office, monthly "Crescent Moon Visibility summary" PDFs for February and March 2026 (Ramaḍān and Shawwāl 1447). Publication pages: <https://www.gov.uk/government/publications/crescent-moon-visibility-for-february-2026> and `...-for-march-2026`. Summary PDFs: `https://assets.publishing.service.gov.uk/media/699c463931713b50fd49c022/Crescent_Moon_Visibility_summary_for_February_2026.pdf` and `.../699c45df31713b50fd49c020/Crescent_Moon_Visibility_summary_for_March_2026.pdf`.
- **Terms:** Crown copyright, published on GOV.UK under the Open Government Licence v3.0.
- **Content:** 24 cities × 4 evenings × 2 months. The visibility category HMNAO prints is mapped to Yallop's codes: Easily Visible → A, Visible in perfect conditions → B, May need optical aid → C, Will need optical aid → D, Not visible with a conventional telescope → E, Not visible → F. Each row also has the local sunset and moonset times. The summaries state that new Moon was at 2026-02-17 12:01 UT and 2026-03-19 01:23 UT.
- **Columns:** `local_date`, `location`, `zone`, `utc_offset_h`, `hmnao_text`, `yallop_category`, `sunset_local`, `moonset_local` (`**:**` means the following day), `sunset_ut`, `moonset_ut`. Times are blank where HMNAO prints none: F and E cells.
- **Caveats:** HMNAO gives no city coordinates, so take them from the atlas and allow for the difference. HMNAO's own note says the Rabat and Dakhla offsets are "subject to confirmation". The extraction relies on the PDF's text order; sunset and moonset sequences were checked for continuity and the categories against HMNAO's prose.
- **Tolerances:** sunset and moonset ±2 min. Category exact, or one step off when the computed q is within 0.03 of a boundary.

## Gaps

- **Odeh Table VI:** the 159 records with V > 8.18 are absent from the available PDF.
- **ICOP observations:** the record pages (astronomycenter.net/icop) give results as Arabic prose and map images, not tables. No per-city predictions or records were extracted. The Odeh table already includes 323 ICOP records up to 2005.
- **Meeus worked examples:** the textbook example values (Meeus 1998, ex. 25.a, 47.a, 12.a and others) are not included, because no citable machine-readable copy was retrieved. Crate tests should cite the book directly. The fixture does include their epochs in the Horizons table.
- **HMNAO world maps:** the daily crescent maps (e.g. `F2026Feb18.pdf`) are images; only the per-city summary table was captured.
- **Umm al-Qura:** a complete month-by-month table (every month, not just the principal days) was not available. van Gent's `ummalqura_table.htm` returned 404.
- **Uncertain eclipse catalogue ΔT:** for future eclipses the catalog's ΔT is an extrapolation. Compare the TD instants, not UT.
