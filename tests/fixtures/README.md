# Validation fixtures

Reference data for regression tests, one directory per topic. Each directory's `README.md` gives the source, units, time scale and frame of its files. [docs/validation.md](../../docs/validation.md) has the full description, licences and suggested tolerances. The scripts that regenerate these files are in [tools/fixtures/](../../tools/fixtures/).

All files are UTF-8 CSV with one header row and no comment lines (plus one JSON file), retrieved 2026-11-11. An empty field means the source gives no value.

| Directory | Contents |
| --- | --- |
| [crescent/](crescent/README.md) | Yallop (1997) and Odeh (2004) sighting tables with computed parameters; published criterion constants |
| [ephemeris/](ephemeris/README.md) | JPL Horizons geocentric and topocentric apparent Sun and Moon |
| [riseset/](riseset/README.md) | USNO rise, set, transit and civil twilight |
| [phases/](phases/README.md) | USNO primary Moon phases |
| [deltat/](deltat/README.md) | Observed ΔT (USNO/IERS, NASA) and Espenak–Meeus polynomial checks |
| [hijri/](hijri/README.md) | Umm al-Qura dates and Saudi announced month starts |
| [eclipses/](eclipses/README.md) | Selected eclipses from NASA's Five Millennium catalogs |
| [predictions/](predictions/README.md) | HMNAO per-city crescent visibility for February and March 2026 |
