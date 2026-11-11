# MoonCalc desktop UI (Tauri)

Vite + TypeScript UI for the Tauri 2 shell in [`crates/mooncalc-tauri`](../../crates/mooncalc-tauri). No UI framework: plain DOM modules and one stylesheet with light and dark themes.

## Run

```sh
npm ci
npm run tauri -- dev     # desktop app with hot reload (Vite on :5173 + cargo run)
npm run dev              # UI only, in any browser, against the mock backend
npm run build            # bundle into dist/ (embedded by release builds)
npm run check            # tsc --noEmit
npm run tauri -- build   # release bundle
```

`npm run tauri` wraps the Tauri CLI so it finds `crates/mooncalc-tauri/tauri.conf.json` from here, and adds `~/.cargo/bin` to `PATH` when it exists.

A standalone debug binary that embeds `dist/` instead of using the dev server: `npm run build`, then `cargo build -p mooncalc-tauri --features custom-protocol`. Plain `cargo build -p mooncalc-tauri` loads `http://localhost:5173`. Add `--features de440` for the JPL DE440 backend (load a `.bsp` file by path in the sidebar).

Debug builds compile the workspace crates without optimisation, so maps are several times slower than in `npm run tauri -- build` or `--release` builds.

## Structure

| Path | Role |
| --- | --- |
| `src/backend/types.ts` | Wire types, mirroring `crates/mooncalc-app/src/dto.rs`. |
| `src/backend/backend.ts` | The `Backend` interface and `BackendError`. |
| `src/backend/tauri.ts` | `TauriBackend`: `invoke` from `@tauri-apps/api`. The only module that imports Tauri. |
| `src/backend/mock.ts` | `MockBackend`: placeholder data for `npm run dev` in a browser (synthetic zones, empty report, no base map). |
| `src/backend/types-calendar.ts`, `types-tables.ts` | Wire types and the `CalendarApi` / `TablesApi` interfaces (`backend.calendar`, `backend.tables`), mirroring `crates/mooncalc-app/src/{calendar,tables}/dto.rs`. |
| `src/backend/tauri-extras.ts`, `mock-extras.ts` | Their Tauri bindings, and in-browser stand-ins (tabular arithmetic, mean motions, canned 2024 eclipses). |
| `src/backend/types-sky.ts`, `tauri-sky.ts`, `mock-sky.ts` | Sky, Trajectory and Moon wire types and the `SkyApi` interface (`backend.sky`), mirroring `crates/mooncalc-app/src/{sky/mod,trajectory,moon/mod}.rs`; its Tauri binding; and a low-precision in-browser model (Astronomical Almanac Sun and Moon, optical libration, a few bright stars). |
| `src/backend/index.ts` | `createBackend()` picks a transport at startup and loads it lazily. |
| `src/state.ts` | Observable app state (date/time, location, criterion, ...). |
| `src/ui/` | Sidebar, tab shell, map canvas (`map-view.ts`, `map-image.ts`, `basemap-layer.ts`), report, earliest sighting, Calendar (`calendar-view.ts`), Ephemeris (`ephemeris-*.ts`), Sky (`sky-view.ts`), Trajectory (`trajectory-view.ts`), Moon (`moon-view.ts`) with shared canvas helpers in `sky-common.ts`, About. |

UI modules import only `src/backend/index.ts`, never a transport. The planned browser frontend adds a `WasmBackend` implementing the same interface.

Maps arrive as base64 zone indices, an optional 16-bit value field and a legend (see `MapResult` in `crates/mooncalc-app/src/dto.rs`). `src/ui/map-image.ts` decodes them and rasterises the zones at device resolution for the current view: where the criterion's zones are thresholds of its value (Yallop q, Odeh V, two-zone margins), each pixel classifies the bilinearly interpolated value, so zone edges are smooth contour lines; elsewhere it takes a bilinear vote of the neighbouring cells. While panning or zooming the last raster is transformed as a preview and redrawn when the view settles. Coastlines and borders come from the `basemap` command (Natural Earth 1:110m, and 1:50m once zoomed in). `src/ui/map-view.ts` adds the graticule, legend, conjunction and best-cell info, a hover readout with zone and value, click-to-set-location (named after the nearest atlas city) and PNG export (`save_map_png` writes to the Downloads folder on the desktop).

The Calendar tab shows a month by the Hijri or the Gregorian month (with month and weekday names in Arabic script), a converter and the month starts of a Hijri year under every calendar. Rule and visibility calendars are computed with the selected ephemeris; the `calendar_*` view commands compute the Hijri years they need on worker threads before answering. The Ephemeris tab has Table (copy as CSV/TSV), Rise/set, Lunar phases, Apsides & nodes, Eclipses (with local circumstances at the app location) and Seasons; clicking a time sets the app date and time. Both keep their own settings in `localStorage` (`mooncalc.calendar.v1`, `mooncalc.ephemeris.*`).

The Sky tab draws the evening (morning) sky of the selected local date from one `sky_scene` call: Sun and Moon samples every two minutes, which the time slider interpolates. Its horizon view shades the sky by the Sun's altitude and shows the Moon phased and turned by the bright-limb angle from the zenith (χ − q), the paths with 10-minute dots and an ARCV/DAZ annotation (wheel zooms, drag pans, double-click resets); the all-sky view puts the zenith at the centre with north up and east left; the star chart plots RA/Dec of date around the Moon with the ecliptic and the horizon. The Trajectory tab shows one local day from `trajectory_day`: an altitude plot with twilight bands, a compass with rise and set azimuths, and readouts at the slider time. The Moon tab rasterises the disc in the browser from the `moon_view` disc parameters (the projection of `mooncalc_app::moon::disc::View`: libration, P or P − q, the subsolar point) with the maria and craters from `moon_features`, plus the lunation's libration curve. Every view is drawn as seen on the sky (east left of north).
