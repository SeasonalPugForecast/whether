# whether plugin SDK for C++23

A header-only client for the whether external display protocol (JSON Lines over
stdio, protocol version 1; see `../schema/protocol-v1.schema.json`). MIT license.

```cmake
add_subdirectory(path/to/whether/sdk/cpp)
target_link_libraries(my_plugin PRIVATE whether::plugin)
```

```cpp
#include <whether/plugin.hpp>

namespace wp = whether::plugin;

int main() {
  wp::Client client;  // stdin/stdout
  return client.run([&](const wp::HostMessage& message) {
    if (std::holds_alternative<wp::Hello>(message)) {
      client.ready("my-plugin");
    } else if (const auto* state = std::get_if<wp::State>(&message)) {
      if (state->view.current) {
        std::println(stderr, "{}: {}", state->view.location, state->view.current->temperature.text);
      }
      // client.send(wp::Refresh{});  // ask for fresh data (within the app's limits)
    }
  });
}
```

Configure the plugin in `~/.config/whether/config.toml`:

```toml
[display]
sidecars = ["exec:my-plugin"]      # or run it as the display: whether --display exec:my-plugin

[display.exec.my-plugin]
command = ["/path/to/my_plugin"]
config = { anything = "passed verbatim in hello" }
```

## Rules

- stdout carries the protocol. Write diagnostics to stderr (`std::println(stderr, …)`)
  or send them with `client.log(…)`; both end up in whether's log.
- Answer `hello` with `ready` within 5 seconds, and exit after `shutdown` (the host
  kills a plugin 2 seconds later).
- Plugins start with a minimal environment: `PATH`, `HOME`, the locale, the
  `XDG_*` directories, the display and session-bus variables, plus the `env` of
  their `[display.exec.<name>]` table. Secrets such as `OPENWEATHER_API_KEY` are
  never passed on.
- Nothing in the API throws. `parse_host_message` returns `std::expected`;
  `Client::run` skips unknown message types and fields, so new host versions
  don't break plugins.
- `ViewModel` models the common fields; everything else is in `view.raw`. Values
  arrive formatted in the user's units (`Quantity::text`); icons are names such as
  `"partly_cloudy_day"`.

## Building and testing

```bash
cmake --preset dev && cmake --build --preset dev && ctest --preset dev
```

`dev` uses GCC with `-Wall -Wextra -Wpedantic -Werror` and ASan + UBSan,
`clang-dev` the same with Clang, `release` an optimized build. From the repository
root, `cargo xtask cpp [--preset …]` does all three steps (and `cargo xtask ci`
runs it). nlohmann/json 3.12.0 comes from its release tarball, pinned by SHA-256,
unless an installed nlohmann_json 3.12 is found. The tests read the golden
messages in `../schema/examples/`, which `cargo xtask schema` writes from the
Rust types.

## Example: `whether-notify`

`examples/notify` builds `whether-notify`, a sidecar that sends desktop
notifications through `notify-send` (spawned with an argv array, never a shell)
when the active location's precipitation chance reaches `min_probability`
(default 60 %) within `lead_minutes` (default 60), or the two-hour outlook expects
rain, and when new alerts appear. Each event notifies once: alerts are told apart
by the provider's `id` (`view.alerts[].id`), so a reworded alert does not notify
again, but one that becomes more severe or ends at least an hour later does
("More severe: …", "Extended: …"); alerts without an id fall back to their text. Without `notify-send` it
logs one warning and keeps running.

```toml
[display]
sidecars = ["exec:notify"]

[display.exec.notify]
command = ["/path/to/sdk/cpp/build/release/examples/notify/whether-notify"]
config = { lead_minutes = 60, min_probability = 60 }
```
