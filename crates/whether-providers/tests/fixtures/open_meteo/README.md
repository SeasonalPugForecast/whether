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

`storm_*.json` (2026-11-14, about 22:50 UTC, `cargo xtask fixtures --provider
open-meteo-storm`) are the storm outlook requests for ZIP 10001, built by
`open_meteo::storm`:
- `storm_point_10001.json`: hourly precipitation, snowfall, gusts and pressure for
  `ecmwf_ifs025`, `gfs_seamless`, `gem_seamless` and `icon_seamless` over 10 days, each
  column suffixed with its model; ICON's last 52 hours are null.
- `storm_ensemble_10001.json`: precipitation and snowfall from the Ensemble API for
  `ecmwf_ifs025` (control and 50 members, suffix `ecmwf_ifs025_ensemble`) and `gfs025`
  (control and 30 members, suffix `ncep_gefs025`) over 10 days. Some ECMWF members
  have hours of -0.1 mm.
- `storm_grid_10001.json`: `pressure_msl` every 6 hours (40 steps) at the 49 points of
  the 7×7 grid around New York, for the four models. The response is an array in
  request order; the echoed points snap to a model grid (two differ: -79 → -79.25 and 45.75 → 46). ICON's last
  7 steps are null.
- `storm_single_run_ecmwf_ifs025_10001.json`: the Single Runs API's `ecmwf_ifs025` run
  of 2026-11-13 00:00 UTC over 10 days, unsuffixed; its hours start at the run.
- `storm_meta_<domain>.json`: `meta.json` of each run domain.

Hand-made storm files: `storm_point_missing_model.synthetic.json` is the point
recording cut to 6 hours without GEM's columns; `storm_grid_nulls.synthetic.json` is
the grid recording cut to 2 steps with ICON null at point 10 and GEM's column missing
at point 11; `storm_error.synthetic.json` has the shape of the 400 the Ensemble API
returns for an unknown model.
