# Ephemeris references (JPL Horizons)

Source: JPL Horizons API (<https://ssd.jpl.nasa.gov/api/horizons.api>), DE441, observer tables, retrieved 2026-11-11 with `tools/fixtures/fetch_fixtures.py horizons-geo horizons-topo`, and 2026-11-13 with `horizons-geo-random`. No usage restrictions known; credit JPL Horizons.

| File | Rows | Content |
| --- | --- | --- |
| `horizons_geocentric.csv` | 88 | Sun and Moon at 44 TT epochs over 1600–2200. Geocentric (`500@399`) airless apparent RA/Dec and ecliptic lon/lat **of date** (IAU76/80 + EOP), apparent range (au, km), angular diameter (″), illuminated %, Sun–observer–target elongation. |
| `horizons_geocentric_random.csv` | 240 | The same columns and quantities as `horizons_geocentric.csv`, at 120 random TT epochs per body over 1900–2100 (seed 20261113, see `geo_epochs_tt_random` in the fetch script). |
| `horizons_topocentric.csv` | 60 | Sun and Moon at five sites, six UT epochs each. Topocentric apparent RA/Dec, azimuth (from north through east) and altitude, both airless and refracted (Horizons' refraction model). Topocentric range and angular diameter. |

- **Time scales:** geocentric epochs are TT (`jd_tt`). `tdb_minus_ut_s` is ΔT only before 1962; from 1962 it is TDB−UTC. Topocentric epochs are Horizons "UT", which is UTC from 1962 (`jd_ut`).
- **Units:** angles in degrees, distances in km (`delta_au` in au).
