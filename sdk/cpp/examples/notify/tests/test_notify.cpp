// The notification decisions against JSON fixtures: the golden state example and
// a synthetic storm (tests/storm.synthetic.json). Nothing is spawned.

#include <filesystem>
#include <fstream>
#include <print>
#include <string>

#include "../notify.hpp"

namespace wp = whether::plugin;

namespace {

int failures = 0;

void check(bool ok, std::string_view what) {
  if (!ok) {
    ++failures;
    std::println(stderr, "FAILED: {}", what);
  }
}

wp::State load(const std::filesystem::path& path) {
  std::ifstream in(path);
  std::string line;
  std::getline(in, line);
  auto message = wp::parse_host_message(line);
  check(message.has_value(), "parsing " + path.string());
  return std::get<wp::State>(*message);
}

void golden_state() {
  const auto state = load(std::filesystem::path(WHETHER_EXAMPLES_DIR) / "state.jsonl");
  const auto now = state.view.hourly.at(0).time;
  // 10–20 % in the next hour: nothing with the defaults.
  check(notify::decide(state, notify::Options{}, now).empty(), "quiet with defaults");
  // Five hours ahead at 50 %: the 16:00 showers (55 %).
  const auto later = notify::decide(state, notify::Options{300, 50}, now);
  check(later.size() == 1, "one precipitation notification");
  if (!later.empty()) {
    check(later[0].kind == notify::Kind::Precipitation, "precipitation kind");
    check(later[0].body.starts_with("55% chance at 16:00"), "body: " + later[0].body);
    check(later[0].location == "New York", "location");
  }
}

void storm() {
  const auto state = load(std::filesystem::path(FIXTURES_DIR) / "storm.synthetic.json");
  const auto now = state.view.hourly.at(0).time;
  const auto all = notify::decide(state, notify::Options{}, now);
  check(all.size() == 2, "precipitation and alert");
  if (all.size() == 2) {
    check(all[0].body.starts_with("80% chance at 12:00"), "hourly wins: " + all[0].body);
    check(all[1].kind == notify::Kind::Alert && all[1].urgent, "urgent alert");
    check(all[1].summary == "Severe Thunderstorm Warning", "alert title");
    check(all[1].id == "storm-1", "alert id");
  }

  // Each event notifies once; a new start hour notifies again.
  notify::Deduper sent;
  check(sent.fresh(all).size() == 2, "first time");
  check(sent.fresh(notify::decide(state, notify::Options{}, now + 600)).empty(), "deduplicated");
  auto moved = state;
  moved.view.hourly.at(1).precip_probability = 10;
  moved.view.hourly.at(2).precip_probability = 90;
  const auto next = sent.fresh(notify::decide(moved, notify::Options{120, 60}, now));
  check(next.size() == 1 && next[0].body.starts_with("90% chance at 13:00"), "new hour");

  // Without hourly chances, the nowcast's text is used.
  auto nowcast_only = state;
  for (auto& h : nowcast_only.view.hourly) {
    h.precip_probability = 0;
  }
  const auto soon = notify::decide(nowcast_only, notify::Options{}, now);
  check(!soon.empty() && soon[0].body == "Rain starting in ~20 min.", "nowcast text");
  // A short lead time ends before the nowcast's rain (the third 15-minute step).
  const auto too_soon = notify::decide(nowcast_only, notify::Options{15, 60}, now);
  check(too_soon.size() == 1 && too_soon[0].kind == notify::Kind::Alert, "nowcast out of reach");
}

// Alerts with an id notify once however their text changes, and again only when
// they become more severe or end at least an hour later; alerts without an id fall
// back to their text.
void alert_identity() {
  const auto state = load(std::filesystem::path(FIXTURES_DIR) / "storm.synthetic.json");
  const auto now = state.view.hourly.at(0).time;
  const auto alerts_of = [&](const wp::State& s) {
    auto all = notify::decide(s, notify::Options{}, now);
    std::erase_if(all, [](const notify::Notification& n) { return n.kind != notify::Kind::Alert; });
    return all;
  };
  const auto with = [&](auto change) {
    auto s = state;
    change(s.view.raw["alerts"][0]);
    return s;
  };
  notify::Deduper sent;
  check(sent.fresh(alerts_of(state)).size() == 1, "first alert");
  const auto reworded = with([](nlohmann::json& a) { a["body"] = "Winds up to 70 mph."; });
  check(sent.fresh(alerts_of(reworded)).empty(), "same id, new text: no repeat");
  const auto other = with([](nlohmann::json& a) { a["id"] = "storm-2"; });
  check(sent.fresh(alerts_of(other)).size() == 1, "a new id notifies");

  const auto end = state.view.raw["alerts"][0]["end"].get<std::int64_t>();
  const auto ending = [&](std::int64_t at) {
    return with([at](nlohmann::json& a) { a["end"] = at; });
  };
  check(sent.fresh(alerts_of(ending(end + 1800))).empty(), "30 minutes later: quiet");
  check(sent.fresh(alerts_of(ending(end - 3600))).empty(), "ending earlier: quiet");
  const auto extended = sent.fresh(alerts_of(ending(end + 3600)));
  check(extended.size() == 1 && extended[0].summary == "Extended: Severe Thunderstorm Warning",
        "an hour later notifies");
  check(sent.fresh(alerts_of(ending(end + 5400))).empty(),
        "measured from the end last notified");
  const auto level = [&](const char* name) {
    return with([&](nlohmann::json& a) {
      a["end"] = end + 3600;
      a["level"] = name;
    });
  };
  check(sent.fresh(alerts_of(level("poor"))).empty(), "less severe: quiet");
  const auto worse = sent.fresh(alerts_of(level("extreme")));
  check(worse.size() == 1 && worse[0].summary.starts_with("More severe: "), "more severe");
  check(sent.fresh(alerts_of(level("extreme"))).empty(), "only once");

  notify::Deduper old_host;
  const auto no_end = with([](nlohmann::json& a) { a.erase("end"); });
  check(old_host.fresh(alerts_of(no_end)).size() == 1, "without an end");
  check(old_host.fresh(alerts_of(state)).empty(), "an end appearing is no extension");

  notify::Deduper by_text;
  const auto no_id = with([](nlohmann::json& a) { a["id"] = nullptr; });
  check(by_text.fresh(alerts_of(no_id)).size() == 1, "without an id");
  check(by_text.fresh(alerts_of(no_id)).empty(), "same text: no repeat");
  const auto no_id_reworded = with([](nlohmann::json& a) {
    a["id"] = nullptr;
    a["body"] = "Winds up to 70 mph.";
  });
  check(by_text.fresh(alerts_of(no_id_reworded)).size() == 1, "without an id, new text notifies");
}

void options() {
  const auto o = notify::Options::from({{"lead_minutes", 30}, {"min_probability", 101}});
  check(o.lead_minutes == 30 && o.min_probability == 60, "invalid values fall back");
  check(notify::Options::from(nlohmann::json::object()).lead_minutes == 60, "defaults");
  check(notify::Options::from(nlohmann::json("x")).min_probability == 60, "not an object");
}

}  // namespace

int main() {
  golden_state();
  storm();
  alert_identity();
  options();
  if (failures == 0) {
    std::println("all notify checks passed");
  }
  return failures == 0 ? 0 : 1;
}
