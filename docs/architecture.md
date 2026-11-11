# Architecture

MoonCalc-rs is a Rust rewrite of Dr. Monzur Ahmed's MoonCalc 6.0, a closed-source DOS program. It is implemented only from published algorithms, never from that program. The headline feature is **world maps of crescent visibility** under Yallop, Odeh and the other classical criteria. Around it the app reproduces MoonCalc's ephemeris tables, Hijri calendars, sky and Moon graphics, and mooncalc.org-style local trajectory views.

## Workspace

| Crate | Role |
| --- | --- |
| `mooncalc-core` | Time scales (JD, ΔT, sidereal time), precession/nutation, frame transforms, topocentric parallax, refraction, the `Ephemeris` trait and the default Meeus/ELP backend. Later: rise/set/transit, phases, conjunctions, apogee/perigee, eclipses, libration. |
| `mooncalc-visibility` | `CrescentGeometry` (ARCL, ARCV, DAZ, W, lag, age ...), its computation from an ephemeris at the right evaluation time, and every sighting criterion as a pure function. |
| `mooncalc-maps` | Visibility grid evaluation, projections, embedded Natural Earth base map, raster (egui texture / PNG) and SVG rendering. |
| `mooncalc-hijri` | Tabular Islamic calendars, Umm al-Qura, and the calendar logic for observed and rule-based months (`ObservedCalendar`, the 29th-day recurrence). The month starts themselves come from `mooncalc-visibility::calendar`. |
| `mooncalc-atlas` | Embedded GeoNames city atlas with search. |
| `mooncalc-spk` | JPL SPK/DAF reader and the DE440 `Ephemeris` backend. Only compiled into the GUI with `--features de440`. |
| `mooncalc-gui` | The egui/eframe desktop app, binary `mooncalc`. |
| `mooncalc-tauri` | Tauri 2 desktop shell, binary `mooncalc-tauri`. It exposes the library crates as Tauri commands to the TypeScript frontend in `frontend/tauri/`. |
| `mooncalc-app` (wave 2) | Application services shared by every frontend: high-level operations such as "compute visibility map", "location report" and "ephemeris table", returning serde DTOs. egui, Tauri and WASM all call this layer, so no logic is duplicated. |
| `mooncalc-wasm` (planned release) | `wasm-bindgen` bindings over `mooncalc-app` for a browser TypeScript frontend in `frontend/web/`. |

Dependency direction: `core` ← `visibility` ← `maps`, `core` ← `hijri` ← `visibility` (its `calendar` module), and `core` ← `atlas`, `spk`. The frontends (`gui`, `tauri`, later `wasm`) depend on everything. Library crates never depend on egui or Tauri.

## Frontends

| Frontend | Status | Notes |
| --- | --- | --- |
| egui (`mooncalc-gui`) | Primary, built now | Native, single binary. |
| Tauri + TypeScript (`mooncalc-tauri`, `frontend/tauri/`) | Built alongside egui | Vite + TypeScript UI in a webview. Heavy computation stays in Rust, behind Tauri commands. |
| TypeScript + WASM (`mooncalc-wasm`, `frontend/web/`) | Planned release addition | The same computations compiled to `wasm32-unknown-unknown` and run in the browser, with grids evaluated in Web Workers. |

**Portability rule (enables WASM):** `mooncalc-core`, `mooncalc-visibility`, `mooncalc-maps`, `mooncalc-hijri` and `mooncalc-atlas` must build for `wasm32-unknown-unknown`. That means no `std::fs`, `std::thread` or `std::time::SystemTime::now` in their default code paths. Put file I/O behind APIs that accept bytes or strings (callers do the reading), and leave parallelism to the caller. Check with `cargo build --target wasm32-unknown-unknown -p <crate>`; the target is installed.

## Conventions

Set out in the `mooncalc-core` crate docs and binding on every crate:

- All angles are radians unless the name ends in `_deg`, `_arcmin`, `_arcsec` or `_hours`. `CrescentGeometry` uses degrees throughout, to match the literature.
- Longitude is east-positive. Azimuth runs from north through east.
- `Instant` carries UT as a Julian Day. Ephemerides take TT Julian Days.
- Distances are in km.
- Ephemerides return **geocentric apparent** positions of date. Topocentric correction is done by the caller.

## Ephemeris backends

- **Default:** Meeus, *Astronomical Algorithms* (2nd ed.). The Sun uses VSOP87 (truncated, ch. 25/32 or a fuller VSOP87D series), the Moon ELP-2000/82 (ch. 47). No data files are needed.
- **`de440` feature:** `De440Ephemeris` reads a user-supplied DE440 or DE440s `.bsp` file at runtime. It reaches apparent geocentric positions through iterated light time, relativistic aberration, and the frame bias × IAU 2006 precession × IAU 1980 nutation rotation (`FrameModel::Iau2006`, the default; `FrameModel::Iau1976` is also available). It implements the same trait, so everything downstream is backend-agnostic. The core's GAST uses GMST 1982, which differs from the IAU 2006 equinox by at most 0.42″ over 1850–2150. That is negligible for visibility, and `de440::sidereal_time_offset` corrects it when needed.

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
| tauri-shell | `crates/mooncalc-tauri`, `frontend/tauri/` |

Wave 2, after wave 1 merges:
- circumstances: best time and `CrescentGeometry` from an ephemeris.
- grid engine: parallel evaluation and the earliest-sighting scan.
- rise/set and events: phases, apogee/perigee, eclipses, libration.
- DE440 apparent-place backend.
- GUI map view and location report.
- Hijri visibility calendars.
- `mooncalc-app` services layer, with the egui and Tauri frontends switched onto it.
- Tauri map view and report.

Wave 3: sky view, star chart, Moon close-up, trajectory view, globe projection and polish, in both desktop frontends.

Planned release addition: `mooncalc-wasm` and the `frontend/web/` TypeScript app.
