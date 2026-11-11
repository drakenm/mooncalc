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

A standalone debug binary that embeds `dist/` instead of using the dev server: `npm run build`, then `cargo build -p mooncalc-tauri --features custom-protocol`. Plain `cargo build -p mooncalc-tauri` loads `http://localhost:5173`.

## Structure

| Path | Role |
| --- | --- |
| `src/backend/types.ts` | Wire types, mirroring `crates/mooncalc-tauri/src/dto.rs`. |
| `src/backend/backend.ts` | The `Backend` interface and `BackendError`. |
| `src/backend/tauri.ts` | `TauriBackend`: `invoke` from `@tauri-apps/api`. The only module that imports Tauri. |
| `src/backend/mock.ts` | `MockBackend`: synthetic data for `npm run dev` in a browser. |
| `src/backend/index.ts` | `createBackend()` picks a transport at startup and loads it lazily. |
| `src/state.ts` | Observable app state (date/time, location, criterion, ...). |
| `src/ui/` | Sidebar, tab shell, map canvas, report, placeholders, About. |

UI modules import only `src/backend/index.ts`, never a transport. The planned browser frontend adds a `WasmBackend` implementing the same interface.

Maps arrive as base64 zone indices plus a legend (see `MapResult` in `dto.rs`). `src/ui/map-image.ts` paints them into a grid-resolution bitmap, and `src/ui/map-view.ts` draws that with zoom, pan, a graticule, a hover readout and click-to-set-location.
