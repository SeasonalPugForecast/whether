# ACIS fixtures

Recorded from data.rcc-acis.org (no key) on 2026-11-02 (about 16:30 UTC, EST) with
`cargo xtask fixtures --provider acis`, which builds every query with the provider's
own planner, for ZIP 10001 with 7 past days, 7 climate days, records and
month-to-date totals.

- `stnmeta_10001.json`: the stations within 0.6° of the place, with their periods.
  The provider chooses Central Park (`NYC`, 7.8 km, since 1869): World Trade Center
  (0.8 km, since 1871) ended in June 2024, and the Wall St heliport (`JRB`, 1.5 km)
  has data only since 2021 and last reported on 31 October.
- `stndata_NYC.json`, `chance_NYC.json`, `records_NYC.json`, `records_NYCthr.json`:
  its days and 1991–2020 normals from 26 October to 8 November (days through 1
  November, all dry), its per-day precipitation counts over 1991–2020 (7 of 30
  for 2 November), its records (83 °F in 1950 and 30 °F in 1887 for 2 November)
  and those of New York's threaded record, which match them.
- The same four for LaGuardia airport (`LGA`, `LGAthr`, 12.8 km), as with
  `[history.acis] station = "LGA"`: normals 60/47 °F and 0.11 in for 2 November,
  records 83 °F (1950) and 29 °F (1965), precipitation on 8 of 30 days.
- `climate_NYC_0712_0719.json` (`cargo xtask fixtures --provider climate`): the
  vacation planner's query at Central Park, 12–19 July over 1995–2024.
