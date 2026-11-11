# Architecture

MoonCalc-rs is a Rust rewrite of Dr. Monzur Ahmed's MoonCalc 6.0, a closed-source DOS program. It is implemented only from published algorithms, never from that program. The headline feature is **world maps of crescent visibility** under Yallop, Odeh and the other classical criteria. Around it the app reproduces MoonCalc's ephemeris tables, Hijri calendars, sky and Moon graphics, and mooncalc.org-style local trajectory views.

## Workspace

| Crate | Role |
| --- | --- |
| `mooncalc-core` | Time scales (JD, ΔT, sidereal time), precession/nutation, frame transforms, topocentric parallax, refraction, the `Ephemeris` trait and the default Meeus/ELP backend. Later: rise/set/transit, phases, conjunctions, apogee/perigee, eclipses, libration. |
| `mooncalc-visibility` | `CrescentGeometry` (ARCL, ARCV, DAZ, W, lag, age ...), its computation from an ephemeris at the right evaluation time, and every sighting criterion as a pure function. |
| `mooncalc-maps` | Visibility grid evaluation, projections, embedded Natural Earth base map, raster (egui texture / PNG) and SVG rendering. |
| `mooncalc-hijri` | Tabular Islamic calendars, Umm al-Qura, and visibility-based calendars (fed month starts by the visibility engine). |
| `mooncalc-atlas` | Embedded GeoNames city atlas with search. |
| `mooncalc-spk` | JPL SPK/DAF reader and the DE440 `Ephemeris` backend. Only compiled into the GUI with `--features de440`. |
| `mooncalc-gui` | The egui/eframe desktop app, binary `mooncalc`. |

Dependency direction: `core` ← `visibility` ← `maps`; `core` ← `hijri`, `atlas`, `spk`; the `gui` depends on everything. Library crates never depend on egui.

## Conventions

Set out in the `mooncalc-core` crate docs and binding on every crate:

- All angles are radians unless the name ends in `_deg`, `_arcmin`, `_arcsec` or `_hours`. `CrescentGeometry` uses degrees throughout, to match the literature.
- Longitude is east-positive. Azimuth runs from north through east.
- `Instant` carries UT as a Julian Day. Ephemerides take TT Julian Days.
- Distances are in km.
- Ephemerides return **geocentric apparent** positions of date. Topocentric correction is done by the caller.

## Ephemeris backends

- **Default:** Meeus, *Astronomical Algorithms* (2nd ed.). The Sun uses VSOP87 (truncated, ch. 25/32 or a fuller VSOP87D series), the Moon ELP-2000/82 (ch. 47). No data files are needed.
- **`de440` feature:** reads a user-supplied DE440 or DE440s `.bsp` file at runtime and applies light time, aberration, precession (IAU 2006) and nutation to reach apparent geocentric positions. It implements the same trait, so everything downstream is backend-agnostic.

## Work packages

Wave 1 runs in parallel. Each package owns the files named here, and edits outside them are limited to adding its own crate's dependencies.

| Package | Owns |
| --- | --- |
| core-foundation | `mooncalc-core` (except the fixed types in `coords.rs` and `ephemeris.rs`, which may only gain items) |
| criteria | `mooncalc-visibility/src/criteria/` |
| maps-render | `mooncalc-maps/src/{projection,basemap,render}.rs`, `assets/basemap/` |
| gui-shell | `mooncalc-gui` |
| atlas | `mooncalc-atlas` |
| hijri | `mooncalc-hijri` |
| spk | `mooncalc-spk` |
| validation | `tests/fixtures/`, `docs/validation.md` |

Wave 2, after wave 1 merges:
- circumstances: best time and `CrescentGeometry` from an ephemeris.
- grid engine: parallel evaluation and the earliest-sighting scan.
- rise/set and events: phases, apogee/perigee, eclipses, libration.
- DE440 apparent-place backend.
- GUI map view and location report.
- Hijri visibility calendars.

Wave 3: sky view, star chart, Moon close-up, trajectory view, globe projection and polish.
