# Local GHI tracker

An offline mirror of the [GitHub issues](https://github.com/drakenm/mooncalc/issues): bugs, features and follow-up work. Read it instead of querying GitHub when you need the backlog. The workflow for keeping it current is in [AGENTS.md](../AGENTS.md#record-follow-up-tasks).

- **Open** and **Closed** list filed issues by `GHI-<issue number>`. The issue number is the ID; never invent one.
- **Untracked** lists deferred work and known limits noted before they had an issue. When one is filed, move it to **Open** under its `GHI-` ID.

## Open

| ID | Area | Item | Found by |
| --- | --- | --- | --- |
| [GHI-4](https://github.com/drakenm/mooncalc/issues/4) | core, app | Rise/set uses observer elevation only for parallax: no horizon dip (`horizon_dip`, `Horizon::UpperLimb { dip }` exist but every caller uses `Horizon::STANDARD`) and sea-level refraction. Add an opt-in dip setting through the services, both frontends and the visibility circumstances; optionally scale refraction with elevation. | rise/set accuracy check |

## Untracked

| Area | Item | Found by |
| --- | --- | --- |
| --- | --- | --- |
| criteria | RGO 1967, Ilyas A/B/C, Shaukat and MABIMS 1995 are not implemented because no primary definition was found. | criteria |
| criteria | Option to evaluate SAAO with the Sun 4° below the horizon (the paper tabulates it). | criteria |
| core | Add GMST 2006 (IERS 2010 eq. 5.32), or apply `de440::sidereal_time_offset` when DE440 is active (max 0.42″). | de440 backend |
| core | ΔT beyond 2026.75 is an extrapolation. Automate refreshing the observed table (`deltat::IERS`) from IERS. | core-foundation, core validation |
| core | Moon light time (about 0.7″) is neglected in the Meeus backend, as in Meeus. | core-foundation |
| atlas | Search for non-Latin script names (Arabic etc.); only Latin alternate names are embedded. | atlas |
| maps | SVG zones are staircase rects; contour tracing would make them smooth. | maps-render |
| maps | Tiny coastline dot on the opposite edge at the antimeridian; raster globe limb antialiasing. | maps-render |
| hijri | KACST revisions to the 1451–1500 projections. (The strict 29th-day Umm al-Qura rule is now `mooncalc_visibility::calendar::UmmAlQuraRule`.) | hijri |
| hijri | The computed Umm al-Qura rule applies the 1423 AH rule before 1420 AH. The 1392–1419 conjunction-time rule and the earlier rules van Gent describes are not implemented. | hijri visibility |
| hijri | Regional visibility calendars use lat/lon boxes that include sea. A land mask (e.g. the maps basemap, passed in as points) would keep ocean-only sightings out. | hijri visibility |
| gui, tauri | No planets or constellation lines in the star charts (egui Sky tab; Tauri Sky, Trajectory and Moon tabs). In Tauri, compass rise/set labels can overlap near due west. | sky + trajectory, tauri sky + moon |
| gui | Arabic month names need a bundled Arabic font plus text shaping in egui. | gui calendar |
| core | Meeus ch. 52 maximum declinations drift up to about 40 min and 100″ in some years; prefer the refined values (documented in `nodes.rs`). | lunar events |
| core | Two missing marginal penumbral lunar eclipses (2027-07-18, 2096-06-06, \|γ\| ≈ 1.575) are beyond the Meeus ch. 54 limits. | lunar events |
| visibility | `MoonAlwaysUp` cells aren't evaluated with Yallop's best time; the earliest-sighting scan is evening-only. | circumstances |
| fixtures | Eclipse tests use `include_str!` paths into the workspace `tests/fixtures/`, which breaks standalone packaging of `mooncalc-core`. | lunar events |
| gui | Map: the zone F / no-crescent boundary is staircased (field masked by `None` cells); globe render is single-threaded (~190 ms at 1600 px); zoom re-renders at up to 3× and then blurs (no cropped viewport). | gui map + report |
| gui | Earliest sighting reports "this zone or better", so zones often share a place/date; it's evening-only. The report uses no horizon dip for elevation (see GHI-4). | gui map + report |
| gui | The egui Calendar tab still lays out its month views, converter and comparison table from `mooncalc_hijri` grids over the engine's `ViewCalendar`, where `CalendarEngine::month`/`convert`/`compare` give the same as DTOs; its `ComputedSpec` also repeats the service's calendar names, descriptions and observer labels. Move the views onto the DTOs. | gui on app services |
| app | The two frontends word a report differently: egui's outcome sentences (with lag and age) and short no-verdict reasons live in `tabs/report.rs`, Tauri shows the service's `statusText` and its own `STATUS_LABEL`; map cell status labels are likewise duplicated. Move one wording into `mooncalc-app` for both. | gui on app services |
| app | A 0.25° map with values is about 4.2 MB of JSON (1.4 MB zones, 2.8 MB 16-bit values as base64). Fine over Tauri IPC (decode about 30 ms), but a binary transport (Tauri `ipc::Response`, WASM shared memory) would halve it. | app services |
| tauri | Native file pickers for the DE440 path and PNG export (needs `tauri-plugin-dialog`); today the path is typed and PNGs go to the Downloads folder. | app services |
| tauri | Map evaluation scales about 6x on 24 threads (0.25° world grid, measured while other builds ran); profile per-cell cost and thread scaling on an idle machine. | app services |
| gui | Ephemeris-tab jobs are keyed on backend choice and path, so `backend::reload()` of the same DE440 file doesn't refresh open views. | gui ephemeris + moon |
| gui | Moon-tab crater coordinates are rounded (about 0.5°) and mare outlines are schematic. | gui ephemeris + moon |
| tauri | Ephemeris table requests are capped at 10,000 rows (1,500 drawn); concurrent calendar requests can compute the same missing year twice. | tauri calendar + ephemeris |
| atlas | GeoNames still lists Namibian winter time (Africa/Windhoek UTC+1/+2) in the atlas's std/DST offsets; tzdb has UTC+2 all year since 2017. Only the search-result offset labels use the GeoNames values; local times use tzdb. Regenerate the atlas when GeoNames updates. | iana time zones |
| tz | Local times use the embedded tzdb 2026e (`jiff-tzdb` 0.1.9). Bump it when IANA publishes rule changes (`tools/tz/README.md`). UTC is taken as UT (< 0.9 s). The egui ephemeris table's daily equation of time and the Tauri mock's Intl zone table (1970–2060) are approximations. | iana time zones |
| tauri | The frontend's zone table (`time_zone_offsets`, 1800–2500) is about 80 KB of JSON for zones with DST, fetched once per zone. A compact array form would shrink it. | iana time zones |
| fixtures | Odeh Table VI is missing 159 naked-eye rows (V > 8.18). ICOP records weren't extractable. | validation |
| licensing | Confirm that committing the Yallop (Crown copyright) and Odeh (Springer) tables as test data is acceptable before the repository goes public. | validation |
| hijri | A computed calendar returns `None` for a whole year when the year before it is not fully covered by the ephemeris (the chain merge needs it), e.g. 1266 AH with DE440s, although its later months are covered. | integration review |
| tauri | The browser draws from service output with small display helpers that mirror Rust: `phaseName`, `compassPoint`, `skyColor` (egui `sky::geometry`), star alt/az with Bennett refraction, the Moon disc projection of `mooncalc_app::moon::disc::View`, and the map's date-line observer note (`src/dateline.ts`, mirroring `mean_time_day_shift` and `mooncalc_app::dateline`; its tests pin the same wording as the Rust tests). Move them behind the WASM backend when it lands, or keep the copies in step. | tauri sky + moon |
| gui | The Sky and Trajectory jobs can't be cancelled inside `NightScene::compute`/`DayScene::compute`, so fast date scrubbing runs one short thread per date (1–8 ms each, measured). The Moon disc raster stays on the UI thread: 18 ms at 512 px in release (41 ms debug), only when the view changes. | gui robustness |
| tauri | `MockBackend`'s sky API is a low-precision model (about 0.3° for the Moon, optical libration only, 25 stars, 10 craters) for `npm run dev`; screenshots of real behaviour need the desktop app. | tauri sky + moon |
| ci | Require the `ci-required` check in branch protection for `master` and `release/v*` (repository settings; `gh` here has no access to the repository). | ci |
| release | Windows builds aren't code-signed (SmartScreen warns), and Linux has no `.rpm` or Flatpak. | release builds |
| ci | The nightly DE440 workflow runs only once `.github/workflows/de440.yml` is on the GitHub default branch (`master` or `main`). Until then, start it manually. | ci |

## Closed

| ID | Area | Item | Resolved by |
| --- | --- | --- | --- |
