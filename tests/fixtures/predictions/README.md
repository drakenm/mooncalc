# Published crescent visibility predictions

`hmnao_city_visibility.csv` (192 rows): HM Nautical Almanac Office monthly summaries for February 2026 and March 2026, the new crescents of Ramaḍān and Shawwāl 1447, published on GOV.UK. Crown copyright, Open Government Licence v3.0. Extracted with `tools/fixtures/extract_hmnao_summary.py` on 2026-11-11.

- Twenty-four cities, each with four evenings per month.
- HMNAO's category text is mapped to Yallop codes A–F.
- Sunset and moonset are given in local time (with `utc_offset_h`) and converted to UT, to the minute.
- HMNAO gives no city coordinates. New Moon fell at 2026-02-17 12:01 UT and 2026-03-19 01:23 UT.
