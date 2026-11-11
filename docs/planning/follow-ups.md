# Follow-ups

GitHub issues can't be filed from this environment yet: the `gh` CLI is authenticated as an account without access to the repository. Until that changes, deferred work is tracked here. Each entry is converted to a `GHI-<n>` entry in `follow-up-tasks.md` once its issue exists.

## Open

| Area | Item | Found by |
| --- | --- | --- |
| criteria | RGO 1967, Ilyas A/B/C, Shaukat and MABIMS 1995 are not implemented because no primary definition was found. | criteria |
| criteria | Option to evaluate SAAO with the Sun 4° below the horizon (the paper tabulates it). | criteria |
| core | Add GMST 2006 (IERS 2010 eq. 5.32), or apply `de440::sidereal_time_offset` when DE440 is active (max 0.42″). | de440 backend |
| core | Automate refreshing the observed ΔT table (`deltat::IERS`). | core-foundation |
| core | Moon light time (about 0.7″) is neglected in the Meeus backend, as in Meeus. | core-foundation |
| atlas | Search for non-Latin script names (Arabic etc.); only Latin alternate names are embedded. | atlas |
| atlas | Time-zone offsets are fixed 2026 values; historical dates need real tz rules. | atlas |
| maps | SVG zones are staircase rects; contour tracing would make them smooth. | maps-render |
| maps | Tiny coastline dot on the opposite edge at the antimeridian; raster globe limb antialiasing. | maps-render |
| hijri | KACST revisions to the 1451–1500 projections. (The strict 29th-day Umm al-Qura rule is now `mooncalc_visibility::calendar::UmmAlQuraRule`.) | hijri |
| hijri | The computed Umm al-Qura rule applies the 1423 AH rule before 1420 AH. The 1392–1419 conjunction-time rule and the earlier rules van Gent describes are not implemented. | hijri visibility |
| hijri | Regional visibility calendars use lat/lon boxes that include sea. A land mask (e.g. the maps basemap, passed in as points) would keep ocean-only sightings out. | hijri visibility |

| gui | Sky/Trajectory tab view options aren't persisted, and their scenes compute on the UI thread (slow with DE440). No planets or constellation lines in the star chart. | sky + trajectory |
| core | ΔT beyond 2026.75 is an extrapolation; refresh the observed table from IERS periodically. | core validation |
| gui | Arabic month names need a bundled Arabic font plus text shaping in egui. | gui calendar |
| core | Meeus ch. 52 maximum declinations drift up to about 40 min and 100″ in some years; prefer the refined values (documented in `nodes.rs`). | lunar events |
| core | Two missing marginal penumbral lunar eclipses (2027-07-18, 2096-06-06, \|γ\| ≈ 1.575) are beyond the Meeus ch. 54 limits. | lunar events |
| visibility | `MoonAlwaysUp` cells aren't evaluated with Yallop's best time; the earliest-sighting scan is evening-only. | circumstances |
| fixtures | Eclipse tests use `include_str!` paths into the workspace `tests/fixtures/`, which breaks standalone packaging of `mooncalc-core`. | lunar events |
| gui | Map: the zone F / no-crescent boundary is staircased (field masked by `None` cells); globe render is single-threaded (~190 ms at 1600 px); zoom re-renders at up to 3× and then blurs (no cropped viewport). | gui map + report |
| gui | Earliest sighting reports "this zone or better", so zones often share a place/date; it's evening-only. The report uses no horizon dip for elevation. | gui map + report |
| gui | Switch the egui GUI onto `mooncalc-app` (its Map/Report tabs call the library crates directly), so both desktop frontends share the services. | app services |
| app | A 0.25° map with values is about 4.2 MB of JSON (1.4 MB zones, 2.8 MB 16-bit values as base64). Fine over Tauri IPC (decode about 30 ms), but a binary transport (Tauri `ipc::Response`, WASM shared memory) would halve it. | app services |
| tauri | Native file pickers for the DE440 path and PNG export (needs `tauri-plugin-dialog`); today the path is typed and PNGs go to the Downloads folder. | app services |
| app | The report anchors the day at civil noon of a fixed UT offset; derive the offset (with DST) from the city's IANA zone in Rust for historical dates. | app services |
| tauri | Map evaluation scales about 6x on 24 threads (0.25° world grid, measured while other builds ran); profile per-cell cost and thread scaling on an idle machine. | app services |
| fixtures | Odeh Table VI is missing 159 naked-eye rows (V > 8.18). ICOP records weren't extractable. | validation |
| licensing | Confirm that committing the Yallop (Crown copyright) and Odeh (Springer) tables as test data is acceptable before the repository goes public. | validation |
| ci | Require the `ci-required` check in branch protection for `master` and `release/v*` (repository settings; `gh` here has no access to the repository). | ci |
| ci | AGENTS.md's release flow refers to a Release workflow that deploys to staging and production. None exists yet, so `ci.yml` only checks. | ci |
| ci | The nightly DE440 workflow runs only once `.github/workflows/de440.yml` is on the GitHub default branch (`master` or `main`). Until then, start it manually. | ci |
