# Eclipses (NASA Five Millennium catalogs)

Source: NASA Eclipse Web Site, Five Millennium Catalogs of solar and lunar eclipses (Espenak & Meeus), pages `SEcat5/SE1901-2000.html`, `SE2001-2100.html`, `LEcat5/LE1901-2000.html` and `LE2001-2100.html`. Retrieved 2026-11-11 with `tools/fixtures/fetch_fixtures.py eclipses`. "Eclipse Predictions by Fred Espenak (NASA's GSFC)". The predictions use VSOP87 and ELP-2000/82.

- `nasa_solar_eclipses.csv` (13 rows) and `nasa_lunar_eclipses.csv` (11 rows) give the instant of greatest eclipse in **TD** (≈ TT), the catalog's ΔT (s), the type, gamma and magnitude(s).
- Solar rows add the point of greatest eclipse (e.g. `25N`, `104W`), Sun altitude, path width (km) and central duration.
- Lunar rows add the phase durations (min) and the sub-lunar point at greatest eclipse.
