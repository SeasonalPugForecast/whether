# MET Norway fixtures

Recorded with `cargo xtask fixtures --provider met-norway` (a contact in
`WHETHER_MET_CONTACT`): `complete_<place>.json` is the forecast for ZIP 10001 and
Tromsø, `sun_10001.json` the day's sunrise and sunset for ZIP 10001.

When the test location moved to New York (2026-11-02) the ZIP 10001 files were not
re-recorded: their coordinates and elevation were rewritten for ZIP 10001, and the weather in
them is still the original 2026-10 recording.
