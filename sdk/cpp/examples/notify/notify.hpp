// whether-notify: what to notify about, decided without side effects so it can be
// tested. main.cpp does the I/O.
#pragma once

#include <algorithm>
#include <array>
#include <chrono>
#include <cstdint>
#include <expected>
#include <iterator>
#include <map>
#include <optional>
#include <ranges>
#include <set>
#include <string>
#include <tuple>
#include <vector>

#include <whether/plugin.hpp>

namespace notify {

namespace wp = whether::plugin;
using nlohmann::json;

// `hello.config`, with defaults for missing or invalid values.
struct Options {
  int lead_minutes = 60;
  int min_probability = 60;

  static Options from(const json& config) {
    Options o;
    const auto pick = [&](const char* key, int lo, int hi, int fallback) {
      const auto* v = wp::detail::field(config, key);
      if (v == nullptr || !v->is_number_integer()) {
        return fallback;
      }
      const auto n = v->get<std::int64_t>();
      return n < lo || n > hi ? fallback : static_cast<int>(n);
    };
    o.lead_minutes = pick("lead_minutes", 1, 24 * 60, o.lead_minutes);
    o.min_probability = pick("min_probability", 1, 100, o.min_probability);
    return o;
  }
};

enum class Kind { Precipitation, Alert };

struct Notification {
  Kind kind;
  std::string location;
  std::int64_t start_hour;  // Unix seconds, truncated to the hour (dedup key)
  std::string summary;
  std::string body;
  bool urgent = false;
  std::optional<std::string> id = std::nullopt;  // the provider's alert id
  int rank = -1;                                  // alert level, 0 good … 5 extreme
  std::optional<std::int64_t> end = std::nullopt; // alert end, Unix seconds
};

// An alert's end must move at least this much later to notify again.
inline constexpr std::int64_t kSignificantExtension = 3600;

namespace detail {

inline std::int64_t hour_of(std::int64_t unix_seconds) {
  using namespace std::chrono;
  const sys_seconds t{seconds{unix_seconds}};
  return floor<hours>(t).time_since_epoch().count() * 3600;
}

inline bool severe(std::string_view level) {
  return level == "poor" || level == "very_poor" || level == "extreme";
}

// A `Level` name's rank, 0 good … 5 extreme; -1 when unknown.
inline int rank(std::string_view level) {
  constexpr std::array<std::string_view, 6> names{"good", "fair", "moderate",
                                                  "poor", "very_poor", "extreme"};
  const auto it = std::ranges::find(names, level);
  return it == names.end() ? -1 : static_cast<int>(it - names.begin());
}

// The first hour within the lead time that reaches the probability threshold.
inline std::optional<Notification> precipitation(const wp::State& s, const Options& o,
                                                 std::int64_t now) {
  const std::int64_t until = now + std::int64_t{o.lead_minutes} * 60;
  // An hour counts from its start, so the current hour is included.
  auto due = s.view.hourly | std::views::filter([&](const wp::HourVm& h) {
               return h.time + 3600 > now && h.time <= until && h.precip_probability &&
                      *h.precip_probability >= o.min_probability;
             });
  if (auto it = std::ranges::begin(due); it != std::ranges::end(due)) {
    const wp::HourVm& h = *it;
    return Notification{Kind::Precipitation, s.view.location, hour_of(h.time),
                        "Precipitation likely in " + s.view.location,
                        std::to_string(*h.precip_probability) + "% chance at " + h.label + ", " +
                            h.temperature.text,
                        false};
  }
  // The two-hour outlook, when it expects precipitation within the lead time.
  const json* nowcast = wp::detail::field(s.view.raw, "nowcast");
  const json* series = nowcast != nullptr ? wp::detail::field(*nowcast, "series_mm") : nullptr;
  const auto text = nowcast != nullptr ? wp::detail::string_at(*nowcast, "text") : std::nullopt;
  if (series == nullptr || !series->is_array() || !text || series->empty()) {
    return std::nullopt;
  }
  // The series spans two hours in equal steps.
  const auto step = 7200 / static_cast<std::int64_t>(series->size());
  for (std::size_t i = 0; i < series->size(); ++i) {
    const auto& mm = (*series)[i];
    const auto at = now + static_cast<std::int64_t>(i) * step;
    if (at > until) {
      break;
    }
    if (mm.is_number() && mm.get<double>() >= 0.1) {
      return Notification{Kind::Precipitation, s.view.location, hour_of(at),
                          "Precipitation soon in " + s.view.location, *text, false};
    }
  }
  return std::nullopt;
}

inline std::vector<Notification> alerts(const wp::State& s, std::int64_t now) {
  std::vector<Notification> out;
  const json* list = wp::detail::field(s.view.raw, "alerts");
  if (list == nullptr || !list->is_array()) {
    return out;
  }
  for (const json& a : *list) {
    const auto title = wp::detail::string_at(a, "title");
    if (!title) {
      continue;
    }
    const auto period = wp::detail::string_at(a, "period").value_or("");
    const auto level = wp::detail::string_at(a, "level").value_or("");
    out.push_back(Notification{Kind::Alert, s.view.location, hour_of(now), *title,
                               (period.empty() ? "" : period + ": ") +
                                   wp::detail::string_at(a, "body").value_or(""),
                               severe(level), wp::detail::string_at(a, "id"), rank(level),
                               wp::detail::integer_at(a, "end")});
  }
  return out;
}

}  // namespace detail

// Everything worth a notification for `state` at `now` (Unix seconds).
inline std::vector<Notification> decide(const wp::State& state, const Options& o,
                                        std::int64_t now) {
  std::vector<Notification> out;
  if (auto p = detail::precipitation(state, o, now)) {
    out.push_back(std::move(*p));
  }
  std::ranges::move(detail::alerts(state, now), std::back_inserter(out));
  return out;
}

// Remembers what was sent so each event notifies once. Precipitation is keyed by
// (location, start hour). An alert with an id notifies once, and again only on a
// significant change: a higher level, or an end at least kSignificantExtension later
// than the one last notified; rewording alone stays quiet. Alerts without
// an id fall back to their title, period and text.
class Deduper {
 public:
  std::vector<Notification> fresh(std::vector<Notification> candidates) {
    std::vector<Notification> out;
    for (auto& n : candidates) {
      if (n.kind == Kind::Alert && n.id) {
        if (significant(n)) {
          out.push_back(std::move(n));
        }
        continue;
      }
      const bool alert = n.kind == Kind::Alert;
      auto key = std::make_tuple(n.location, n.kind, alert ? std::int64_t{0} : n.start_hour,
                                 alert ? n.summary + "\n" + n.body : std::string{});
      if (seen_.insert(std::move(key)).second) {
        out.push_back(std::move(n));
      }
    }
    return out;
  }

 private:
  // What was last notified for an alert.
  struct Sent {
    int rank;
    std::optional<std::int64_t> end;
  };

  // Whether `n` (an alert with an id) is new or changed significantly since it was
  // last notified; marks the summary of a change.
  bool significant(Notification& n) {
    auto [it, inserted] =
        alerts_.try_emplace(std::make_pair(n.location, *n.id), Sent{n.rank, n.end});
    if (inserted) {
      return true;
    }
    Sent& sent = it->second;
    if (!sent.end) {
      sent.end = n.end;  // an older host sent no end: nothing to compare yet
    }
    const bool stronger = n.rank > sent.rank;
    const bool extended = n.end && sent.end && *n.end >= *sent.end + kSignificantExtension;
    if (!stronger && !extended) {
      return false;
    }
    n.summary = (stronger ? "More severe: " : "Extended: ") + n.summary;
    sent.rank = std::max(sent.rank, n.rank);
    if (extended) {
      sent.end = n.end;
    }
    return true;
  }

  std::set<std::tuple<std::string, Kind, std::int64_t, std::string>> seen_;
  std::map<std::pair<std::string, std::string>, Sent> alerts_;
};

}  // namespace notify
