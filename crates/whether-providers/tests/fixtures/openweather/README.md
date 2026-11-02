# OpenWeather fixtures

When the test location moved to New York (2026-11-02) these files were not
re-recorded: their place names, ZIP, coordinates and the city's population were rewritten
for ZIP 10001, and the weather in them is still the original 2026-10 recording.

- `*.synthetic.json` are hand-made from the field lists in OpenWeather's API docs
  (<https://openweathermap.org/current>, <https://openweathermap.org/forecast5>,
  <https://openweathermap.org/api/air-pollution>) for ZIP 10001 (New York, NY) on
  2026-05-18. Tests that assert hand-picked values use them.
- `weather_10001.json`, `forecast_10001_cnt40.json`, `forecast_10001_cnt8.json` and
  `air_pollution_10001.json` were recorded on 2026-05-18 with `cargo xtask fixtures
  --provider openweather`; the recorder checks that no file contains the key.
- `error_401_*.json` are real response bodies recorded on 2026-05-18; they contain no key.
- `onecall4_*.synthetic.json` are hand-made from the One Call 4.0 reference
  (<https://openweathermap.org/api/one-call-4>) for ZIP 10001 on 2026-05-18 (current,
  three 20-hour pages of `/timeline/1h`, `/timeline/1day` and `/timeline/15min`).
  Their daily `dt` follows the recorded convention (00:00 UTC of the local date).
- `onecall4_current_alerts_10001.synthetic.json` is the synthetic `current` naming one
  alert by id, and `onecall4_alert_10001.synthetic.json` that alert's `/alert/{id}`
  details, shaped after the reference's alert example (an NWS Wind Advisory). No
  alert was active for 10001 when the real files were recorded.
- `onecall4_*_10001.json` (without `.synthetic`) were recorded on 2026-06-28 with
  `cargo xtask fixtures --provider openweather-onecall4`. The `prev`/`next` paging
  URLs carry `appid=***`; the recorder scrubs the key and refuses to write a file
  that still contains it. The `1h` timeline ends 48 hours ahead, so page 2 holds
  only 8 hours.
- `key_rejected.synthetic.json` is hand-made after the documented 401 body
  (`{"cod":401,"message":"…"}`), for the key check.
- `error_401_onecall4_subscription.json` is the real body returned on 2026-05-18 before
  the "One Call by Call" subscription was active; it contains no key.
- `geocode_zip_10001.json` and `geocode_direct_new_york.json` were recorded on
  2026-06-28 with `cargo xtask fixtures --provider openweather-geocode`. The ZIP
  endpoint names Manhattan (the township around the ZIP's centroid) and gives no
  state; the direct endpoint finds New York, New York.
