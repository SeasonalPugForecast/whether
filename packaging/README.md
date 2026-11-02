# Packaging

- `whether.desktop`: the desktop entry (`whether gui`).
- `icons/hicolor/`: the app icon, the Meteocons `partly-cloudy-day` icon by Bas Milius
  (MIT, see `crates/whether-gui/assets/icons/meteocons/LICENSE`). The PNGs are rendered
  by `cargo xtask icons raster`.
- `man/whether.1` and `completions/`: generated from the CLI by
  `cargo xtask dist --assets`. A test fails when they are stale.
- `deb/`: copyright notices and the kiosk postinst for `cargo deb -p whether`
  (metadata in `apps/whether/Cargo.toml`; variants `kiosk` and `headless`).
- `systemd/whether-kiosk.service` and `systemd/whether.sysusers`: the Raspberry Pi
  kiosk system service and its user. Kiosk builds are embedded use of Slint; see
  `deb/copyright-kiosk`.
- `systemd/whether-daemon.service`: a user unit for `whether daemon`, the shared
  fetch service that displays started with `--attach` connect to.
- `cross/Dockerfile.<target>`: Debian bookworm images for the ARM builds
  (`cargo xtask dist --target aarch64-unknown-linux-gnu`): cross toolchain, the
  libraries the GUI and kiosk link. xtask builds
  them with plain `docker build`; `Cross.toml` names them. Package a cross build
  with `CARGO_TARGET_DIR=target/cross cargo deb -p whether --target <target>
  --variant kiosk --no-build --no-strip`.
