# Architecture

MoonCalc-rs is a Rust rewrite of Dr. Monzur Ahmed's MoonCalc 6.0, a closed-source DOS program. It is implemented only from published algorithms, never from that program. The headline feature is **world maps of crescent visibility** under Yallop, Odeh and the other classical criteria. Around it the app reproduces MoonCalc's ephemeris tables, Hijri calendars, sky and Moon graphics, and mooncalc.org-style local trajectory views.

## Workspace

| Crate | Role |
| --- | --- |
| `mooncalc-core` | Time scales (JD, ΔT, sidereal time), precession/nutation, frame transforms, topocentric parallax, refraction, the `Ephemeris` trait and the default Meeus/ELP backend. `events` (rise/set/transit, twilight, phases), `seasons`, `apsides`, `nodes`, `libration`, `eclipses` (Meeus ch. 54 plus Besselian-element refinement and local circumstances) and `tables` (ephemeris table rows). |
| `mooncalc-visibility` | `CrescentGeometry` (ARCL, ARCV, DAZ, W, lag, age ...), its computation from an ephemeris at the right evaluation time, and every sighting criterion as a pure function. |
| `mooncalc-maps` | Visibility grid evaluation, projections, embedded Natural Earth base map, raster (egui texture / PNG) and SVG rendering. |
| `mooncalc-hijri` | Tabular Islamic calendars, Umm al-Qura, and the calendar logic for observed and rule-based months (`ObservedCalendar`, the 29th-day recurrence). The month starts themselves come from `mooncalc-visibility::calendar`. |
| `mooncalc-atlas` | Embedded GeoNames city atlas with search, and civil time zones (`tz::Zone`): IANA zones with their DST and historical rules from the embedded IANA Time Zone Database, or fixed offsets. |
| `mooncalc-spk` | JPL SPK/DAF reader and the DE440 `Ephemeris` backend. Only compiled into the GUI with `--features de440`. |
| `mooncalc-gui` | The egui/eframe desktop app, binary `mooncalc`. |
| `mooncalc-tauri` | Tauri 2 desktop shell, binary `mooncalc-tauri`. It exposes the `mooncalc-app` services as Tauri commands to the TypeScript frontend in `frontend/tauri/`, adding threads (map rows in parallel), timing and DE440 loading by path. |
| `mooncalc-app` | Application services shared by every frontend, from request DTO to response DTO (serde, camelCase JSON): ephemeris selection (Meeus truncated/full, DE440 behind the `de440` feature), criteria with legends, visibility maps (prepare → evaluate row blocks → assemble, so callers parallelise; PNG export), the location report under every criterion, atlas search, the earliest-sighting scan, simplified base-map polylines, Hijri calendars (`calendar`: month views, converter, month-start comparison; computed calendars one Hijri year at a time) ephemeris tables and event lists (`tables`), the evening/morning sky of a sighting with the bright stars of date (`sky`), the 24-hour Sun and Moon day (`trajectory`) and the Moon close-up (`moon`: physical ephemeris, disc parameters, libration curve, IAU Gazetteer craters and schematic maria). `sky` and `moon` also hold the frontend-independent sky and Moon math (parallactic and bright-limb angles, scenes, the Yale BSC star list, the disc projection), which the egui Sky, Trajectory and Moon tabs import. Portable (builds for `wasm32-unknown-unknown`): no threads, clocks or file I/O in default paths. Both desktop frontends use it: the Tauri shell through commands, the egui GUI directly for the report, maps and the earliest-sighting scan, the Ephemeris-tab event lists and the computed Hijri calendars (`CalendarEngine`), adding only threads, wording and layout. The WASM bindings are to follow. |
| `mooncalc-wasm` (planned release) | `wasm-bindgen` bindings over `mooncalc-app` for a browser TypeScript frontend in `frontend/web/`. |

Dependency direction: `core` ← `visibility` ← `maps`, `core` ← `hijri` ← `visibility` (its `calendar` module), and `core` ← `atlas`, `spk`. The frontends (`gui`, `tauri`, later `wasm`) depend on everything. Library crates never depend on egui or Tauri.

## Frontends

| Frontend | Status | Notes |
| --- | --- | --- |
| egui (`mooncalc-gui`) | Primary, built now | Native, single binary. Calls the `mooncalc-app` services in-process. Expensive views (maps, tables, calendars, the Sky and Trajectory scenes) compute on background jobs (`jobs`, cancel-and-replace; map rows spread over every core); view options persist in `Settings` with serde defaults. |
| Tauri + TypeScript (`mooncalc-tauri`, `frontend/tauri/`) | Built alongside egui | Vite + TypeScript UI in a webview. Heavy computation stays in Rust, behind Tauri commands. |
| TypeScript + WASM (`mooncalc-wasm`, `frontend/web/`) | Planned release addition | The same computations compiled to `wasm32-unknown-unknown` and run in the browser, with grids evaluated in Web Workers. |

**Portability rule (enables WASM):** `mooncalc-core`, `mooncalc-visibility`, `mooncalc-maps`, `mooncalc-hijri`, `mooncalc-atlas` and `mooncalc-app` must build for `wasm32-unknown-unknown`. That means no `std::fs`, `std::thread` or `std::time::SystemTime::now` in their default code paths. Put file I/O behind APIs that accept bytes or strings (callers do the reading), and leave parallelism to the caller. Check with `cargo build --target wasm32-unknown-unknown -p <crate>`; the target is installed.

## Conventions

Set out in the `mooncalc-core` crate docs and binding on every crate:

- All angles are radians unless the name ends in `_deg`, `_arcmin`, `_arcsec` or `_hours`. `CrescentGeometry` uses degrees throughout, to match the literature.
- Longitude is east-positive. Azimuth runs from north through east.
- `Instant` carries UT as a Julian Day. Ephemerides take TT Julian Days.
- Distances are in km.
- Ephemerides return **geocentric apparent** positions of date. Topocentric correction is done by the caller.
- **Time zones.** Local times come from a `mooncalc_atlas::Zone`: an IANA zone (the default once a city is chosen) or a fixed offset. The offset is taken at each instant, so times on either side of a DST change, and on dates under older rules, are right. A local day runs from one local midnight to the next, so it lasts 23 or 25 hours when the clocks change. A local time that was skipped moves forward by the gap; one that occurred twice takes its first occurrence. App services take an optional `timeZone` with the fixed `tzOffsetHours` as fallback. Every `UtDateTime` they return carries the `offsetHours` in force at that instant, and `time_zone_offsets` gives the frontends a zone's transitions for any instant. The rules come from `jiff` with `jiff-tzdb` (tzdb 2026e), built without OS access, so wasm32 uses the same data. `tools/tz/README.md` explains updates.
- **Dates of maps and reports.** A world map of a date shows each place on its own local evening (or morning) of that date, anchored at 12h local mean time, so its date line is the 180° meridian, as on HMNAO's maps (`mooncalc_maps::engine`). The location report follows the observer's civil date (civil noon in their time zone). The two agree wherever the zone offset is within 12 hours of mean solar time. East of 180° in UTC+13/+14 (Samoa, Tonga, the Line Islands) the civil date runs a day ahead, so the map for 10 March shows their civil 11 March. `CalendarDate::mean_time_date` gives the map date for a civil date. The report then carries a `mapDateNote`, and both frontends state the map's convention and note when the evening under the observer marker is not their civil one (`mooncalc_app::dateline`).

## Ephemeris backends

- **Default:** Meeus, *Astronomical Algorithms* (2nd ed.). The Sun uses VSOP87 (truncated, ch. 25/32 or a fuller VSOP87D series), the Moon ELP-2000/82 (ch. 47). No data files are needed.
- **`de440` feature:** `De440Ephemeris` reads a user-supplied DE440 or DE440s `.bsp` file at runtime. It reaches apparent geocentric positions through iterated light time, relativistic aberration, and the frame bias × IAU 2006 precession × IAU 1980 nutation rotation (`FrameModel::Iau2006`, the default; `FrameModel::Iau1976` is also available). It implements the same trait, so everything downstream is backend-agnostic. The core's GAST uses GMST 1982, which differs from the IAU 2006 equinox by at most 0.42″ over 1850–2150. That is negligible for visibility, and `de440::sidereal_time_offset` corrects it when needed.
- **Coverage:** outside an ephemeris' `valid_range` (1849-12-26 to 2150-01-21 for DE440s) positions are NaN. `circumstances::compute_with` then returns `Outcome::OutsideEphemeris` (status `outside-ephemeris`) rather than a polar or "not visible" result. The egui tabs check the span they need, plus the look-back and look-ahead of their searches (a lunation for phases and the Moon's age; for views computed by a service, the service's own `*COVERAGE_MARGIN_DAYS`), with `ActiveEphemeris::span_problem`, and show a notice instead of computing. Computed Hijri calendars give the months of a partly covered year whose data lies inside the coverage (`mooncalc_app::calendar::start_covered`).

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
