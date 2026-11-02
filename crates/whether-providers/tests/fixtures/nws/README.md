# NWS fixtures

Recorded from api.weather.gov (no key; a User-Agent naming the app).

- `alerts_10001.json` (2026-11-02): no active alerts at New York.
- `alerts_flood.json` (2026-07-28): Florida's active alerts, a coastal flood
  statement and four flood warnings, one without an end.
- `points_10001.json`, `gridpoint_OKX_33_42.json`, `forecast_OKX_33_42.json`
  (2026-11-02, `cargo xtask fixtures --provider nws`): the point lookup for ZIP 10001
  (office OKX, grid 33,42), then the raw gridpoint forecast (170 KB, every layer as
  ISO 8601 intervals in WMO units, updated 11:19:51 UTC) and the twice-daily forecast
  it names. The same command records `open_meteo/forecast_full_10001_with_nws.json`
  at the same time.
- `stations_OKX_33_42.json` and `observation_KNYC.json` (2026-11-02, `cargo xtask
  fixtures --provider nws-observation`, from the recorded point lookup): the stations
  near the grid cell (KNYC, Central Park, first) and KNYC's latest observation
  (14:51 UTC, 15 °C, clear, no wind reported).
