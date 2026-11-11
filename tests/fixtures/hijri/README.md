# Hijri calendar dates

Source: R.H. van Gent, *The Umm al-Qura Calendar of Saudi Arabia*, Utrecht University (<https://webspace.science.uu.nl/~gent0113/islam/>), pages `ummalqura_principal.htm` and `ummalqura_adjust.htm`. Retrieved 2026-11-11 with `tools/fixtures/fetch_fixtures.py hijri`. © R.H. van Gent; factual dates, cited for testing.

| File | Rows | Content |
| --- | --- | --- |
| `ummalqura_principal_days.csv` | 31 | Umm al-Qura Gregorian dates of 1 Muḥarram, 12 Rabīʿ I, 1 Ramaḍān, 1 Shawwāl and 10 Dhu ʾl-Ḥijja, 1420–1450 AH |
| `saudi_announced_vs_ummalqura.csv` | 135 | 1422–1448 AH: Umm al-Qura dates against the dates announced by the Saudi High Judiciary Council (via Fatwa-Online, as compiled by van Gent). A blank announced date means none is tabulated. |

Dates are ISO Gregorian civil dates: the day that begins at midnight after the Hijri day has started at sunset.
