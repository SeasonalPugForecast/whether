# whether-sim

Renders the firmware's display pages on the host, per board profile in
`firmware/boards.toml`, so layouts can be checked before flashing.

```bash
cargo run -p whether-sim -- --board cyd --page daily --out cyd-daily.png
cargo run -p whether-sim --features window -- --board esp32-wroom-spi --window
```

`--fixture <cache.json>` shows a cache file written by `whether` instead of the
built-in sample. In the window, the arrow keys change page and Esc or q quits.
Goldens: `apps/whether-sim/tests/golden/` (`UPDATE_GOLDEN=1` rewrites them).
