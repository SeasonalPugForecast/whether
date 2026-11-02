// Protocol tests against the golden examples that `cargo xtask schema` writes from
// the Rust types (sdk/schema/examples), so both sides agree on every message.

#include <whether/plugin.hpp>

#include <filesystem>
#include <fstream>
#include <print>
#include <sstream>
#include <string>
#include <vector>

namespace wp = whether::plugin;
using nlohmann::json;

namespace {

int failures = 0;

void check(bool ok, std::string_view what) {
  if (!ok) {
    ++failures;
    std::println(stderr, "FAILED: {}", what);
  }
}

std::vector<std::string> lines(const std::string& name) {
  std::ifstream in(std::filesystem::path(WHETHER_EXAMPLES_DIR) / name);
  check(in.good(), "opening " + name);
  std::vector<std::string> out;
  for (std::string line; std::getline(in, line);) {
    out.push_back(line);
  }
  return out;
}

template <class T>
const T* as(const std::expected<wp::HostMessage, wp::Error>& m) {
  return m ? std::get_if<T>(&*m) : nullptr;
}

void host_examples_parse() {
  const auto hello = wp::parse_host_message(lines("hello.jsonl").at(0));
  const auto* h = as<wp::Hello>(hello);
  check(h != nullptr, "hello parses");
  if (h != nullptr) {
    check(h->protocol == wp::kProtocolVersion, "hello protocol");
    check(h->plugin_name == "notify", "hello plugin name");
    check(h->config.value("lead_minutes", 0) == 60, "hello config");
  }

  const auto state = wp::parse_host_message(lines("state.jsonl").at(0));
  const auto* s = as<wp::State>(state);
  check(s != nullptr, "state parses");
  if (s != nullptr) {
    check(s->seq == 1 && s->active == 0, "state seq and active");
    check(s->view.location == "New York", "state location");
    check(s->view.attribution.find("Open-Meteo") != std::string::npos, "state attribution");
    check(s->view.current.has_value(), "state current");
    if (s->view.current) {
      check(s->view.current->temperature.text == "15°", "current temperature text");
      check(s->view.current->temperature.value.has_value(), "current temperature value");
      check(!s->view.current->icon.empty(), "current icon");
    }
    check(!s->view.hourly.empty() && !s->view.daily.empty(), "hourly and daily rows");
    check(s->view.hourly.at(0).label == "Now", "first hour is now");
    check(s->snapshots.is_null(), "no snapshots unless requested");
    check(s->view.raw.contains("details"), "raw keeps unmodelled fields");
  }

  check(as<wp::Shutdown>(wp::parse_host_message(lines("shutdown.jsonl").at(0))) != nullptr,
        "shutdown parses");
}

void plugin_examples_are_not_host_messages() {
  for (const char* name : {"ready.jsonl", "command.jsonl", "log.jsonl"}) {
    for (const auto& line : lines(name)) {
      const auto m = wp::parse_host_message(line);
      check(!m && m.error().kind == wp::ErrorKind::UnknownType,
            std::string("plugin message in ") + name);
    }
  }
}

void malformed_lines_have_kinds() {
  const auto kind = [](std::string_view line) {
    const auto m = wp::parse_host_message(line);
    return m ? std::optional<wp::ErrorKind>{} : std::optional{m.error().kind};
  };
  check(kind("{") == wp::ErrorKind::Json, "truncated JSON");
  check(kind("") == wp::ErrorKind::Json, "empty line");
  check(kind("[1,2]") == wp::ErrorKind::Shape, "array");
  check(kind(R"({"name":"x"})") == wp::ErrorKind::Shape, "no type");
  check(kind(R"({"type":"state"})") == wp::ErrorKind::Shape, "state without fields");
  check(kind(R"({"type":"state","seq":-1,"active":0,"view":{}})") == wp::ErrorKind::Shape,
        "negative seq");
  check(kind(R"({"type":"hello","protocol":"1","app_version":"x","plugin_name":"y"})") ==
            wp::ErrorKind::Shape,
        "protocol as a string");
  check(kind(R"({"type":"hello","protocol":2,"app_version":"x","plugin_name":"y"})") ==
            wp::ErrorKind::UnsupportedProtocol,
        "protocol 2");
  check(kind(R"({"type":"reboot"})") == wp::ErrorKind::UnknownType, "unknown type");
  check(kind(std::string(wp::kMaxLine + 1, ' ')) == wp::ErrorKind::Json, "too long");
  // Unknown fields are ignored.
  check(!kind(R"({"type":"shutdown","reason":"update","v2":{"x":1}})").has_value(),
        "extra fields");
}

void client_writes_what_rust_reads() {
  std::istringstream in(lines("hello.jsonl").at(0) + "\n" + R"({"type":"future_thing"})" +
                        "\nnot json\n" + lines("state.jsonl").at(0) + "\n" +
                        lines("shutdown.jsonl").at(0) + "\n" + lines("state.jsonl").at(0) + "\n");
  std::ostringstream out;
  wp::Client client(in, out);
  int states = 0;
  int seen = 0;
  const int code = client.run([&](const wp::HostMessage& m) {
    ++seen;
    if (std::holds_alternative<wp::Hello>(m)) {
      client.ready("whether-notify");
    } else if (std::holds_alternative<wp::State>(m)) {
      ++states;
      client.send(wp::Refresh{0});
      client.send(wp::SelectLocation{1});
      client.send(wp::Quit{});
      client.log(wp::LogLevel::Info, "notify-send not found; notifications are off");
    }
  });
  check(code == 0, "run returns 0");
  check(seen == 3 && states == 1, "unknown and malformed lines skipped, stop after shutdown");

  std::vector<json> written;
  std::istringstream produced(out.str());
  for (std::string line; std::getline(produced, line);) {
    written.push_back(json::parse(line, nullptr, false));
  }
  std::vector<json> expected;
  for (const char* name : {"ready.jsonl", "command.jsonl", "log.jsonl"}) {
    for (const auto& line : lines(name)) {
      expected.push_back(json::parse(line));
    }
  }
  check(written == expected, "ready, commands and log match the Rust encoding");

  // A handler returning false stops the loop.
  std::istringstream again(lines("hello.jsonl").at(0) + "\n" + lines("state.jsonl").at(0) + "\n");
  std::ostringstream ignored;
  wp::Client stopping(again, ignored);
  int calls = 0;
  stopping.run([&](const wp::HostMessage&) {
    ++calls;
    return false;
  });
  check(calls == 1, "false stops the loop");

  // Refresh without a location sends null, which Rust reads as "all".
  std::ostringstream all;
  std::istringstream none;
  wp::Client(none, all).send(wp::Refresh{});
  check(json::parse(all.str())["command"]["location"].is_null(), "refresh all");
}

}  // namespace

int main() {
  host_examples_parse();
  plugin_examples_are_not_host_messages();
  malformed_lines_have_kinds();
  client_writes_what_rust_reads();
  if (failures == 0) {
    std::println("all protocol checks passed");
  }
  return failures == 0 ? 0 : 1;
}
