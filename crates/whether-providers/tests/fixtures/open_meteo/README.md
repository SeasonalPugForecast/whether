# Open-Meteo fixtures

Recorded from the live API (no key needed) on 2026-05-18 with `cargo xtask fixtures
--provider open-meteo`, which builds every URL with the provider's own planner.

- `geocode_*.json` come from `https://geocoding-api.open-meteo.com/v1/search` with the
  queries in their names; `geocode_none.json` is the response to `zzqqxxy`.
- `forecast_esp32_10001.json` uses the ESP32 panel profile (current, 12 hours, 5 days).
- `forecast_full_<place>.json` use `DataRequirements::full()` for ZIP 10001, Tromsø,
  Sydney, Honolulu and Reykjavík (different offsets, latitudes and hemispheres).
- `air_quality_10001.json` is the current air quality for ZIP 10001.
- The ZIP 10001 files (`forecast_esp32_10001`, `forecast_full_10001`,
  `air_quality_10001`, `geocode_10001_us`) were re-recorded on 2026-11-02 (16:30
  UTC) when the test location moved to New York.
- `error_400_latitude.json` is the error body for latitude 952.52.
- `forecast_full_10001_with_nws.json` (2026-11-02, recorded by `cargo xtask fixtures
  --provider nws`) is `DataRequirements::full()` for ZIP 10001 at the time of the NWS
  forecast fixtures, to supplement them in tests.
- `forecast_models_hrrr_nbm_ecmwf.json` (2026-11-02, `cargo xtask fixtures --provider
  open-meteo-models`) is `DataRequirements::full()` for ZIP 10001 with
  `models = ["ncep_hrrr_conus", "ncep_nbm_conus", "ecmwf_ifs025"]`: every variable is
  suffixed with its model, HRRR's hours end after 66 steps, `current` is unsuffixed.

Hand-made files end in `.synthetic.json`: `forecast_nulls` has a null in every column and
`forecast_length_mismatch` has a column shorter than its `time` array.

`history_past_10001.json` (2026-11-02, `cargo xtask fixtures --provider open-meteo-history`)
is the archived model forecast for ZIP 10001 from 26 October to 1 November (daily high, low,
precipitation and snowfall), the past days of the history fallback. The ERA5
archive for 1991–2020 (277 KB) is not recorded: `tests/open_meteo_history.rs` generates
one in its shape.
