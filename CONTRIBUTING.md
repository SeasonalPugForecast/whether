# Contributing

Issues and pull requests are welcome at
<https://github.com/SeasonalPugForecast/whether>.

## Build and test

```sh
cargo xtask ci          # the gate: fmt, clippy -D warnings, tests, no_std checks, cargo-deny
cargo test -p <crate>   # one crate
cargo insta review      # review changed snapshots
```

The gate must pass on every commit. `cargo xtask hooks install` runs it before each push.

## Commits

One commit per working unit: the smallest complete change that compiles, carries its own tests and
passes the gate. Sign every commit off (`git commit -s`), which certifies the
[Developer Certificate of Origin](https://developercertificate.org/). Write the subject as
`<component>: short description`, where the component is one of `workspace`, `core`, `providers`,
`config`, `display`, `runtime`, `tui`, `gui`, `panel`, `line`, `proto`, `cli`, `sim`, `fw`,
`xtask`, `sdk-cpp`, `packaging`, `docs`, `ios`. The body says what the change adds, briefly.

## Rules the code relies on

- `whether-core`, `whether-providers`, `whether-config` (without `std`), `whether-tui` (without
  `host`), `whether-panel` and `whether-gui` (with `mcu`) are `#![no_std]` plus `alloc`. Do not add
  std-only dependencies to them; use `libm` for float math.
- Only `whether-core::view` converts units or formats values. Displays never do.
- Providers are sans-IO. Network I/O lives in `whether-runtime` and the firmware's `net` module.
- Tests never touch the network. Recorded responses are `*.json`; hand-made ones are
  `*.synthetic.json`; live tests are `#[ignore]` and named `live_*`.
- No secrets in the repository: API keys come from the environment, and URLs are logged only
  through `redact_url`.
- Every graphical and terminal display shows the data attribution.
- The `ViewModel`, `WeatherSnapshot`, the config schema and the plugin protocol are contracts:
  change them together with their documentation and schema files (`cargo xtask schema`).
- No `unwrap` or `expect` outside tests without an `// INVARIANT:` comment.

The firmware is a separate workspace: build it only with `cargo xtask fw build`.
