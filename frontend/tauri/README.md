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
| `src/backend/index.ts` | `createBackend()` picks a transport at startup and loads it lazily. |
| `src/state.ts` | Observable app state (date/time, location, criterion, ...). |
| `src/ui/` | Sidebar, tab shell, map canvas (`map-view.ts`, `map-image.ts`, `basemap-layer.ts`), report, earliest sighting, placeholders, About. |

UI modules import only `src/backend/index.ts`, never a transport. The planned browser frontend adds a `WasmBackend` implementing the same interface.

Maps arrive as base64 zone indices, an optional 16-bit value field and a legend (see `MapResult` in `crates/mooncalc-app/src/dto.rs`). `src/ui/map-image.ts` decodes them and rasterises the zones at device resolution for the current view: where the criterion's zones are thresholds of its value (Yallop q, Odeh V, two-zone margins), each pixel classifies the bilinearly interpolated value, so zone edges are smooth contour lines; elsewhere it takes a bilinear vote of the neighbouring cells. While panning or zooming the last raster is transformed as a preview and redrawn when the view settles. Coastlines and borders come from the `basemap` command (Natural Earth 1:110m, and 1:50m once zoomed in). `src/ui/map-view.ts` adds the graticule, legend, conjunction and best-cell info, a hover readout with zone and value, click-to-set-location (named after the nearest atlas city) and PNG export (`save_map_png` writes to the Downloads folder on the desktop).
