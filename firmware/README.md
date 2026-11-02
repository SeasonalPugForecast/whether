# whether firmware (ESP32)

A separate Cargo workspace for the boards in `boards.toml`: `whether-fw` (the
device application) and `fw-config` (its configuration, validated and embedded at
build time). It reuses the host crates `whether-core`, `whether-providers`,
`whether-tui` and `whether-panel` as `no_std` libraries.

## Toolchain (once)

```bash
cargo install espup@0.18.0 espflash@4.6.0 esp-generate@1.4.0 --locked
espup install            # the "esp" Rust toolchain (Xtensa) + GCC; writes ~/export-esp.sh
```

`~/export-esp.sh` is POSIX sh. `cargo xtask fw` reads its `export` lines itself,
so it works from fish without sourcing anything.

## Build, flash, monitor

```bash
cargo xtask fw build --board esp32-wroom-spi [--release]
cargo xtask fw size  --board esp32-wroom-spi --release   # fails above 90 % of the app partition
cargo xtask fw flash --board esp32-wroom-spi --release   # needs the board on USB; then monitors
cargo xtask fw monitor --board esp32-wroom-spi
```

Flashing needs access to the serial port; on Arch that means the `uucp` group.
The boot log prints a banner with the version, board, display, locations and
heap use, and never a secret.

## Configuration

Copy `whether-fw.example.toml` to `whether-fw.toml` (gitignored), or point
`WHETHER_FW_CONFIG` at a file. `build.rs` validates it with the same rules as
Linux plus the firmware's own (explicit coordinates, the board being built, a
compiled-in display, a Wi-Fi network name), resolves `{ env = "…" }` secrets and
embeds the result. The flashed image therefore contains the Wi-Fi password and
any API key. Without a `whether-fw.toml` the example is used, with placeholder
Wi-Fi values, so the image builds but cannot join a network.

## How the skeleton was made

Generated with esp-generate 1.4.0 and then moved into this workspace (the
profiles, `.cargo/` and `rust-toolchain.toml` went to the workspace root, the chip
became the `chip-esp32` feature):

```bash
esp-generate --headless -o esp32 -o esp32-wroom-32 -o unstable-hal -o alloc -o wifi -o embassy -o log -o esp-backtrace whether-fw
```

Keep the generated parts (linker setup in `build.rs`, the app descriptor, the
heap and scheduler setup in `main.rs`) as generated; extend around them.

## Wiring notes

For the `esp32-wroom-32` module, the generated `main.rs` lists GPIO16 and GPIO20
as "in use by some feature of the module and should not be used". A common
wiring puts the display's DC line on GPIO16, so pick another pin
for DC. Also avoid GPIO6–11 (flash) and remember that GPIO34–39 are input-only.
Pins go into `boards.toml` once the wiring is confirmed.
