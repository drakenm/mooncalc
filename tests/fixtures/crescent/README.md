# Crescent sightings and criteria

| File | Rows | Source |
| --- | --- | --- |
| `yallop1997_table4.csv` | 295 | Yallop (1997), NAO Technical Note 69, Table 4 (<https://www.astronomycenter.net/pdf/yallop_1997.pdf>). Crown copyright, cited for testing. |
| `odeh2004_table6.csv` | 578 of 737 | Odeh (2004), *Exp. Astron.* 18:39–64, doi:10.1007/s10686-005-9002-5, Table VI, transcribed from the scanned PDF. © Springer, cited for testing. Records with V > 8.18 are absent from the PDF. |
| `criteria_published_constants.json` | – | Yallop and Odeh criterion constants and thresholds. |

- Angles are in degrees. Longitude is east positive. DAZ = azimuth of Sun − azimuth of Moon.
- Yallop: ARCL and ARCV are geocentric and airless; W′ is topocentric, in arcminutes; values are at Yallop's best time Ts + (4/9)·Lag. `jd_newmoon` is the conjunction JD. Dates are local civil dates.
- Odeh: ARCV, DAZ and ARCL are topocentric and airless; `w_arcsec` is the topocentric width in **arcseconds**; values are at best time `jd_best`, assumed UT. `date` is printed as dd-mm-yyyy.
- Printed values are kept as published, including known inconsistencies. See [docs/validation.md](../../../docs/validation.md#crescent-sightings-and-criteria-crescent).
