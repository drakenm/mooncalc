# Rise, set and transit (USNO)

`usno_rstt.csv` (558 rows): USNO Astronomical Applications API `rstt/oneday` with `tz=0` and no height (sea level). Retrieved 2026-11-11 with `tools/fixtures/fetch_fixtures.py usno-rstt`. U.S. Government work, public domain.

- Six sites (Mecca, Greenwich, Cape Town, Kuala Lumpur, Los Angeles, Tromsø) on 10 dates each, plus 4 high-latitude dates for Tromsø.
- `time_ut` is HH:MM in UT (UT1), rounded to the minute, for events within the UT calendar day `date_ut`.
- `event` is one of `rise`, `set`, `upper_transit`, `lower_transit`, `civil_twilight_begin`, `civil_twilight_end`, or a continuous state (`continuously_above_horizon`, `continuously_below_horizon`, `continuously_above_twilight_limit`). `fracillum` rows hold the Moon's illuminated percentage.
- USNO definitions: the upper limb is on the horizon, with 34′ refraction, a 16′ solar semi-diameter and topocentric lunar parallax. Civil twilight is the Sun's centre at −6°.
