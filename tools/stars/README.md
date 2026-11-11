# Bright-star data

`build_stars.py` generates `crates/mooncalc-gui/src/sky/stars_data.rs`, the bright-star table drawn by the egui Sky tab (horizon view, all-sky view and star chart).

## Source

The Yale Bright Star Catalogue, 5th revised edition: D. Hoffleit and W. H. Warren Jr., *The Bright Star Catalogue*, 5th rev. ed., Yale University Observatory, 1991 (CDS catalogue V/50). The catalogue is in the public domain. The script reads the fixed-width `bsc5.dat` distributed by the Harvard-Smithsonian Center for Astrophysics at <http://tdc-www.harvard.edu/catalogs/bsc5.html> (`bsc5.dat.gz`, 590 228 bytes compressed, fetched 2026-11-11).

## Regenerating

From the repository root, with Python 3.10 or newer (standard library only):

```sh
python -I tools/stars/build_stars.py --download
```

This downloads `bsc5.dat.gz` into `tools/stars/download/` (gitignored, never committed) and rewrites the Rust table. To convert a copy you already have, pass `--input path/to/bsc5.dat.gz` (or the uncompressed `bsc5.dat`) instead of `--download`. The catalogue is static, so a rebuild reproduces the same file.

## Selection and fields

- Every star with V < 3.5 that has a position (284 stars), sorted brightest first.
- HR 5958 (T CrB) is excluded: BSC5 records the recurrent nova at its 1866 outburst peak, V = 2.0, but it normally shines near V = 10.
- Positions are the catalogue's J2000.0 (FK5) RA and Dec, kept to 0.1 s and 1″. Proper motion is not applied; the largest among these stars (Arcturus, 2.3″/yr) moves it 0.06° per century, which is invisible at chart scale. The GUI precesses and nutates the J2000 positions to the true equator and equinox of date with `mooncalc_core::precession::j2000_to_true_of_date_matrix` (IAU 1976 precession, IAU 1980 nutation); annual aberration (≤ 20″) is neglected.
- `bv` is the catalogue B−V colour index (0 when blank), used to tint the star symbols.
- `label` is the Bayer letter (as a Greek letter, with any superscript number) and constellation, or the Flamsteed number and constellation; empty for stars with neither.
- `name` is the IAU Working Group on Star Names proper name for about 70 of the brightest stars, from a hand-kept table in the script.
