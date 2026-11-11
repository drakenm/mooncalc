# Moon phases (USNO)

`usno_moon_phases.csv` (1683 rows): USNO API `moon/phases/year` for 2000–2030 and for 1700, 1800 and 1900. Retrieved 2026-11-11 with `tools/fixtures/fetch_fixtures.py usno-phases`. Public domain.

Columns are `date_ut`, `time_ut` (HH:MM Universal Time, rounded to the minute) and `phase` (`New Moon`, `First Quarter`, `Full Moon`, `Last Quarter`). Phases are geocentric instants at which the apparent longitude difference Moon − Sun is 0°, 90°, 180° or 270°.
