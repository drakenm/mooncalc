# Atlas data tools

These scripts build the city atlas embedded in `mooncalc-atlas`.

## Regenerating `assets/atlas/cities.tsv`

From the repository root, with Python 3.10 or newer (standard library only):

```sh
python -I tools/atlas/build_atlas.py --download
```

This downloads `cities15000.zip`, `countryInfo.txt`, `admin1CodesASCII.txt` and `timeZones.txt` from <https://download.geonames.org/export/dump/> into `tools/atlas/download/`, which is gitignored and never committed. It then writes `assets/atlas/cities.tsv`. To convert files you already have, pass `--input DIR` instead of `--download`. `--cities cities5000` selects a larger dump, which makes the file about 4.2 MB instead of 2.3 MB.

GeoNames updates its dumps daily, so a rebuild reproduces the format and the selection rules, not identical bytes. The `#generated` header records the snapshot date. After regenerating, run `cargo test -p mooncalc-atlas`. The tests check row counts and well-known search and nearest-city results.

`gen_fold_table.py` regenerates `crates/mooncalc-atlas/src/fold_table.rs`, the table used for diacritic-insensitive search. It is built from Unicode decomposition data and only needs re-running to extend the covered character ranges.

## Selection rules

- All rows of the chosen dump, skipping rows whose country or time zone is missing from `countryInfo.txt` or `timeZones.txt` (none in the 2026-11-11 snapshot).
- Elevation is the GeoNames `elevation` field, then `dem` when that is empty. A `dem` of -9999 (no data) becomes 0.
- Coordinates are rounded to 4 decimal places, about 11 m.
- Alternate names are kept only for places with population of at least 50 000. Only Latin-script names starting with a capital letter are kept, excluding airport-style all-capital codes and duplicates after case and diacritic folding. Up to 12 per place are kept, shortest first, so exonyms such as Mecca, Bombay, Peking and Rangoon stay searchable.
- Standard offset is the `timeZones.txt` raw offset. The DST offset is whichever of the January and July offsets differs from it, or equals it where DST is not observed. Both describe the snapshot year and are only labels (search results). Local times use the city's IANA zone through `mooncalc_atlas::tz`, with the full rules from the IANA Time Zone Database (see `tools/tz/README.md`). `tz::tests::every_atlas_zone_is_known` checks that every atlas zone resolves and that its 2026 offsets match these columns.

## File format (version 1)

UTF-8, LF line endings, tab-separated. Lines starting with `#` are comments, except that the parser reads `#source`.

```text
#mooncalc-atlas	1
#source	GeoNames cities15000 (https://www.geonames.org/), CC BY 4.0
#generated	YYYY-MM-DD
@countries	N
CC	ISO3	Country name                   (sorted by code)
@admin1	N
country_index	Admin-1 name
@timezones	N
IANA id	std_offset_h	dst_offset_h
@cities	N
name	ascii_name	alt|names	country_index	admin1_index	lat	lon	elevation_m	population	tz_index
```

`ascii_name` is empty when it equals `name`. `alt|names` and `admin1_index` may be empty. Cities are grouped by country index and sorted by population, descending, within each country. `Atlas::by_country` relies on this ordering.

## Licence

GeoNames data is licensed under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). Credit GeoNames (<https://www.geonames.org/>) wherever the data is shown. The crate exports the credit text as `mooncalc_atlas::ATTRIBUTION`.
