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
| hijri | Strict Umm al-Qura rule that tests the 29th day of the running month; KACST revisions to the 1451–1500 projections. | hijri |
| core | ΔT beyond 2026.75 is an extrapolation; refresh the observed table from IERS periodically. | core validation |
| gui | Arabic month names need a bundled Arabic font plus text shaping in egui. | gui calendar |
| tauri | The mock backend duplicates the Rust stubs, and the fallback legend colours may differ from the criteria colours. Both resolve when `mooncalc-app` lands. | tauri-shell |
| core | Meeus ch. 52 maximum declinations drift up to about 40 min and 100″ in some years; prefer the refined values (documented in `nodes.rs`). | lunar events |
| core | Two missing marginal penumbral lunar eclipses (2027-07-18, 2096-06-06, \|γ\| ≈ 1.575) are beyond the Meeus ch. 54 limits. | lunar events |
| visibility | `MoonAlwaysUp` cells aren't evaluated with Yallop's best time; the earliest-sighting scan is evening-only. | circumstances |
| fixtures | Eclipse tests use `include_str!` paths into the workspace `tests/fixtures/`, which breaks standalone packaging of `mooncalc-core`. | lunar events |
| fixtures | Odeh Table VI is missing 159 naked-eye rows (V > 8.18). ICOP records weren't extractable. | validation |
| licensing | Confirm that committing the Yallop (Crown copyright) and Odeh (Springer) tables as test data is acceptable before the repository goes public. | validation |
