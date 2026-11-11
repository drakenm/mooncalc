# Documentation map

| Document | What it covers |
| --- | --- |
| [Architecture](architecture.md) | Workspace layout, shared conventions, crate ownership, and the work-package plan. |
| [Base map data](../assets/basemap/README.md) | Natural Earth source, the MCBM binary format, and how to regenerate it. |
| [Atlas data](../tools/atlas/README.md) | GeoNames conversion, embedded atlas format and selection rules. |
| [Time-zone data](../tools/tz/README.md) | The embedded IANA Time Zone Database (jiff, jiff-tzdb), how to update it, and why no custom table. |
| [Hijri data](../tools/hijri/README.md) | Umm al-Qura table sources, checksums and regeneration. |
| [Bright-star data](../tools/stars/README.md) | Yale Bright Star Catalogue source, selection rules and regeneration of the Sky tab's star table. |
| [Tauri frontend](../frontend/tauri/README.md) | Running and building the Tauri + TypeScript app, and the backend transport layer. |
| [Continuous integration](ci.md) | GitHub Actions jobs (`ci-required`), Linux build packages, the DE440 kernel workflow, action pins, and reproducing CI locally. |
| [Validation](validation.md) | Reference datasets in `tests/fixtures/` (sources, licences, units, time scales), how `tools/fixtures/` regenerates them, and suggested test tolerances. |
| [Follow-ups](planning/follow-ups.md) | Deferred work and known limits, until GitHub issues can be filed. |

Agent workflow rules are in [AGENTS.md](../AGENTS.md).
