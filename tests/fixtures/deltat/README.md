# ΔT

| File | Rows | Source | Units |
| --- | --- | --- | --- |
| `usno_observed_deltat.csv` | 685 | USNO/IERS `maia.usno.navy.mil/ser7/historic_deltat.data` (1657.0–1972.5, half-yearly) and `deltat.data` (1 January values for 1974–2026). Public domain. | decimal year; TT−UT1 in s |
| `nasa_deltat_historical.csv` | 28 | Espenak, NASA, `SEhelp/deltat2004.html` Table 1 (Morrison & Stephenson 2004), −500 to 1950 | s, with standard error as printed |
| `nasa_deltat_recent.csv` | 11 | same page, Table 2 (Astronomical Almanac 2006), 1955–2005 | s |
| `espenak_meeus_polynomial_checks.csv` | 42 | **Computed** by `tools/fixtures/espenak_meeus_deltat.py` from the published Espenak & Meeus polynomials (`SEhelp/deltatpoly2004.html`); not independently published values | s; input is decimal year y |

NASA data credit: "Eclipse Predictions by Fred Espenak (NASA's GSFC)". All files retrieved 2026-11-11.
