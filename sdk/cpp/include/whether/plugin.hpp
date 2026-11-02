// whether plugin SDK: a header-only C++23 client for the whether external display
// protocol (JSON Lines over stdio, protocol version 1). MIT license.
//
// The host writes `hello`, `state` and `shutdown` messages to the plugin's stdin,
// one JSON object per line; the plugin answers on stdout with `ready`, `command`
// and `log`. Diagnostics go to stderr, because stdout is the protocol channel.
//
// Nothing in this API throws: parse errors come back as `std::unexpected(Error)`,
// unknown fields are ignored, and `Client::run` skips unknown message types so
// plugins keep working when the host adds messages.
#pragma once

#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <expected>
#include <iostream>
#include <optional>
#include <print>
#include <string>
#include <string_view>
#include <type_traits>
#include <utility>
#include <variant>
#include <vector>

#include <nlohmann/json.hpp>

namespace whether::plugin {

inline constexpr int kProtocolVersion = 1;
// The longest line either side accepts, newline excluded (1 MiB).
inline constexpr std::size_t kMaxLine = std::size_t{1} << 20;

// A formatted value: the number in display units (if known), its text and unit.
struct Quantity {
  std::optional<double> value;
  std::string text;
  std::string unit;
};

// Icons are snake_case names such as "partly_cloudy_day" (whether-core IconId).
struct CurrentVm {
  Quantity temperature, feels_like, high, low;
  std::string condition_label;
  std::string icon;
};

struct HourVm {
  std::int64_t time = 0;  // Unix seconds
  std::string label;
  std::string icon;
  Quantity temperature;
  std::optional<int> precip_probability;
};

struct DayVm {
  std::string label;
  std::string icon;
  Quantity high, low;
  std::optional<int> precip_probability;
};

// The parts of the view model the SDK models; `raw` holds the whole object for
// everything else (details, air quality, nowcast, alerts, ...).
struct ViewModel {
  std::string location;
  std::string attribution;
  std::string theme;
  std::optional<CurrentVm> current;
  std::optional<std::string> summary;
  std::vector<HourVm> hourly;
  std::vector<DayVm> daily;
  nlohmann::json raw;
};

struct Hello {
  int protocol = 0;
  std::string app_version, plugin_name;
  nlohmann::json config;  // [display.exec.<name>].config, an object
};

struct State {
  std::uint64_t seq = 0;
  std::size_t active = 0;
  ViewModel view;
  nlohmann::json snapshots;  // null unless the plugin set send_snapshot
};

struct Shutdown {};

using HostMessage = std::variant<Hello, State, Shutdown>;

enum class ErrorKind { Json, Shape, UnsupportedProtocol, UnknownType };

struct Error {
  ErrorKind kind;
  std::string message;
};

struct Refresh {
  std::optional<std::size_t> location;  // all locations when empty
};
struct SelectLocation {
  std::size_t index = 0;
};
struct Quit {};

using Command = std::variant<Refresh, SelectLocation, Quit>;

enum class LogLevel { Error, Warn, Info, Debug };

namespace detail {

using json = nlohmann::json;

// The member `key` of object `j`, or null when `j` is not an object or lacks it.
inline const json* field(const json& j, std::string_view key) {
  if (!j.is_object()) {
    return nullptr;
  }
  const auto it = j.find(std::string{key});
  return it == j.end() ? nullptr : &*it;
}

inline std::optional<std::string> string_at(const json& j, std::string_view key) {
  const json* f = field(j, key);
  if (f == nullptr || !f->is_string()) {
    return std::nullopt;
  }
  return f->get_ref<const std::string&>();
}

inline std::optional<std::uint64_t> unsigned_at(const json& j, std::string_view key) {
  const json* f = field(j, key);
  if (f == nullptr || !f->is_number_unsigned()) {
    return std::nullopt;
  }
  return f->get<std::uint64_t>();
}

inline std::optional<std::int64_t> integer_at(const json& j, std::string_view key) {
  const json* f = field(j, key);
  if (f == nullptr || !f->is_number_integer()) {
    return std::nullopt;
  }
  return f->get<std::int64_t>();
}

inline std::optional<double> number_at(const json& j, std::string_view key) {
  const json* f = field(j, key);
  if (f == nullptr || !f->is_number()) {
    return std::nullopt;
  }
  return f->get<double>();
}

inline std::optional<int> percent_at(const json& j, std::string_view key) {
  const auto v = integer_at(j, key);
  if (!v || *v < 0 || *v > 100) {
    return std::nullopt;
  }
  return static_cast<int>(*v);
}

inline Quantity quantity_at(const json& j, std::string_view key) {
  const json* f = field(j, key);
  if (f == nullptr) {
    return {};
  }
  return Quantity{number_at(*f, "value"), string_at(*f, "text").value_or(""),
                  string_at(*f, "unit").value_or("")};
}

inline std::unexpected<Error> shape(std::string message) {
  return std::unexpected(Error{ErrorKind::Shape, std::move(message)});
}

inline ViewModel parse_view(const json& v) {
  ViewModel out;
  out.raw = v;
  if (const json* location = field(v, "location")) {
    out.location = string_at(*location, "name").value_or("");
  }
  out.attribution = string_at(v, "attribution").value_or("");
  out.theme = string_at(v, "theme").value_or("");
  out.summary = string_at(v, "summary");
  if (const json* c = field(v, "current"); c != nullptr && c->is_object()) {
    out.current = CurrentVm{quantity_at(*c, "temperature"), quantity_at(*c, "feels_like"),
                            quantity_at(*c, "high"),        quantity_at(*c, "low"),
                            string_at(*c, "condition_label").value_or(""),
                            string_at(*c, "icon").value_or("")};
  }
  if (const json* hourly = field(v, "hourly")) {
    if (const json* hours = field(*hourly, "hours"); hours != nullptr && hours->is_array()) {
      for (const json& h : *hours) {
        out.hourly.push_back(HourVm{integer_at(h, "time").value_or(0),
                                    string_at(h, "label").value_or(""),
                                    string_at(h, "icon").value_or(""),
                                    quantity_at(h, "temperature"),
                                    percent_at(h, "precip_probability")});
      }
    }
  }
  if (const json* daily = field(v, "daily")) {
    if (const json* days = field(*daily, "days"); days != nullptr && days->is_array()) {
      for (const json& d : *days) {
        out.daily.push_back(DayVm{string_at(d, "label").value_or(""),
                                  string_at(d, "icon").value_or(""), quantity_at(d, "high"),
                                  quantity_at(d, "low"), percent_at(d, "precip_probability")});
      }
    }
  }
  return out;
}

inline std::string_view level_name(LogLevel level) {
  switch (level) {
    case LogLevel::Error:
      return "error";
    case LogLevel::Warn:
      return "warn";
    case LogLevel::Info:
      return "info";
    case LogLevel::Debug:
      return "debug";
  }
  return "info";
}

inline json command_json(const Command& cmd) {
  return std::visit(
      [](const auto& c) -> json {
        using T = std::decay_t<decltype(c)>;
        if constexpr (std::is_same_v<T, Refresh>) {
          json j = {{"kind", "refresh"}};
          j["location"] = c.location ? json(*c.location) : json(nullptr);
          return j;
        } else if constexpr (std::is_same_v<T, SelectLocation>) {
          return {{"kind", "select_location"}, {"index", c.index}};
        } else {
          return {{"kind", "quit"}};
        }
      },
      cmd);
}

}  // namespace detail

// Parses one line from the host (without its newline).
[[nodiscard]] inline std::expected<HostMessage, Error> parse_host_message(std::string_view line) {
  using detail::json;
  if (line.size() > kMaxLine) {
    return std::unexpected(Error{ErrorKind::Json, "line exceeds 1 MiB"});
  }
  const json j = json::parse(line, nullptr, /*allow_exceptions=*/false);
  if (j.is_discarded()) {
    return std::unexpected(Error{ErrorKind::Json, "not valid JSON"});
  }
  if (!j.is_object()) {
    return detail::shape("not a JSON object");
  }
  const auto type = detail::string_at(j, "type");
  if (!type) {
    return detail::shape("missing string field `type`");
  }
  if (*type == "hello") {
    const auto protocol = detail::integer_at(j, "protocol");
    const auto app_version = detail::string_at(j, "app_version");
    const auto plugin_name = detail::string_at(j, "plugin_name");
    if (!protocol || !app_version || !plugin_name) {
      return detail::shape("hello needs protocol, app_version and plugin_name");
    }
    if (*protocol != kProtocolVersion) {
      return std::unexpected(Error{ErrorKind::UnsupportedProtocol,
                                   "host speaks protocol " + std::to_string(*protocol)});
    }
    const json* config = detail::field(j, "config");
    return Hello{static_cast<int>(*protocol), *app_version, *plugin_name,
                 config != nullptr ? *config : json::object()};
  }
  if (*type == "state") {
    const auto seq = detail::unsigned_at(j, "seq");
    const auto active = detail::unsigned_at(j, "active");
    const json* view = detail::field(j, "view");
    if (!seq || !active || view == nullptr || !view->is_object()) {
      return detail::shape("state needs seq, active and a view object");
    }
    const json* snapshots = detail::field(j, "snapshots");
    return State{*seq, static_cast<std::size_t>(*active), detail::parse_view(*view),
                 snapshots != nullptr ? *snapshots : json(nullptr)};
  }
  if (*type == "shutdown") {
    return Shutdown{};
  }
  return std::unexpected(Error{ErrorKind::UnknownType, "unknown message type `" + *type + "`"});
}

// Reads host messages and writes plugin messages.
class Client {
 public:
  explicit Client(std::istream& in = std::cin, std::ostream& out = std::cout)
      : in_(in), out_(out) {}

  // Answers `hello`.
  void ready(std::string_view name) {
    write({{"type", "ready"}, {"name", std::string{name}}, {"protocol", kProtocolVersion}});
  }

  void send(const Command& cmd) {
    write({{"type", "command"}, {"command", detail::command_json(cmd)}});
  }

  void log(LogLevel level, std::string_view message) {
    write({{"type", "log"},
           {"level", std::string{detail::level_name(level)}},
           {"message", std::string{message}}});
  }

  // Calls `on_message` for each host message until `shutdown` (passed on too) or
  // the end of input, and returns the exit code (0). A handler that returns
  // `bool` stops the loop by returning false. Malformed lines are reported on
  // stderr and skipped; unknown message types are skipped silently.
  template <class Handler>
  int run(Handler&& on_message) {
    std::string line;
    while (std::getline(in_, line)) {
      if (!line.empty() && line.back() == '\r') {
        line.pop_back();
      }
      auto message = parse_host_message(line);
      if (!message) {
        if (message.error().kind != ErrorKind::UnknownType) {
          std::println(stderr, "whether plugin: ignoring a host line: {}", message.error().message);
        }
        continue;
      }
      const bool shutdown = std::holds_alternative<Shutdown>(*message);
      if constexpr (std::is_invocable_r_v<bool, Handler&, const HostMessage&> &&
                    !std::is_void_v<std::invoke_result_t<Handler&, const HostMessage&>>) {
        if (!on_message(std::as_const(*message))) {
          return 0;
        }
      } else {
        on_message(std::as_const(*message));
      }
      if (shutdown) {
        return 0;
      }
    }
    return 0;
  }

 private:
  void write(const nlohmann::json& j) {
    // Invalid UTF-8 is replaced rather than thrown on.
    out_ << j.dump(-1, ' ', false, nlohmann::json::error_handler_t::replace) << '\n';
    out_.flush();
  }

  std::istream& in_;
  std::ostream& out_;
};

}  // namespace whether::plugin
