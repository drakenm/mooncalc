# Time-zone data

`mooncalc_atlas::tz` evaluates civil time zones from the IANA Time Zone Database (<https://www.iana.org/time-zones>, public domain). No script in this repository compiles the data. It comes from two pinned crates, both declared in the workspace `Cargo.toml`:

- `jiff-tzdb` embeds the whole database, compiled by `zic` into TZif and concatenated (about 207 KB). Its `VERSION` (for example `2026e`) is exposed as `mooncalc_atlas::tz::TZDB_VERSION` and shown on the About tab.
- `jiff` parses the TZif data and evaluates it: offsets, transitions, abbreviations, and the gap and fold rules for local-to-UT conversion. It is built with `default-features = false` and `features = ["alloc", "tzdb-bundle-always"]`, so it never reads the operating system's zoneinfo or registry, and native and `wasm32-unknown-unknown` builds give the same answers.

## Updating to a new tzdata release

1. Check crates.io for a `jiff-tzdb` release that carries the new tzdata. It must be at least 14 days old and have no RustSec advisories (see `AGENTS.md`). Its `tzname.rs` names the tzdata version.
2. Bump the exact pin `jiff-tzdb = "=0.1.N"` in the workspace `Cargo.toml`, then run `cargo update -p jiff-tzdb --precise 0.1.N`. `jiff` accepts any `0.1.x` `jiff-tzdb`, so `jiff` itself only needs a bump when a fix requires it. Pin that one exactly as well.
3. Run `cargo test -p mooncalc-atlas`. `tz::tests` pin published transitions (London BST 2024, New York 2006 and 2024, Riyadh, Kolkata, Havana, Lord Howe). `every_atlas_zone_is_known` checks that every atlas zone still resolves and that its 2026 offsets match the GeoNames snapshot. A country that changed its rules will show up there: update the test's exception list, or regenerate the atlas (`tools/atlas/README.md`).
4. Record the new version in the commit message, for example `chore(deps): tzdata 2027a (jiff-tzdb 0.1.10)`.

## Why not compile our own table

A custom compiled table would need a `zic`-compatible compiler for the tzdata source format, its rules and `Zone` continuation lines, plus a POSIX-TZ evaluator for dates after the last listed transition. Restricting the table to the atlas's 356 zones would save at most about 100 KB over the full bundled database. `jiff` is widely used and maintained, and an update then needs only a version bump.
