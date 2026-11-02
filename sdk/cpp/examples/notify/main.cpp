// whether-notify: a sidecar plugin that sends desktop notifications when
// precipitation is near or new weather alerts appear. MIT license.
//
//   [display]
//   sidecars = ["exec:notify"]
//
//   [display.exec.notify]
//   command = ["whether-notify"]
//   config = { lead_minutes = 60, min_probability = 60 }

#include <spawn.h>
#include <sys/wait.h>

#include <cerrno>
#include <chrono>
#include <cstring>
#include <expected>
#include <print>
#include <string>
#include <vector>

#include "notify.hpp"

extern char** environ;

namespace {

namespace wp = whether::plugin;

enum class SendError { Missing, Failed };

// Runs notify-send with an argv array (no shell) and waits for it.
std::expected<void, SendError> send(const notify::Notification& n) {
  std::vector<std::string> args{"notify-send", "--app-name=whether",
                                n.urgent ? "--urgency=critical" : "--urgency=normal", "--",
                                n.summary, n.body};
  std::vector<char*> argv;
  for (auto& a : args) {
    argv.push_back(a.data());
  }
  argv.push_back(nullptr);
  pid_t pid = 0;
  const int rc = posix_spawnp(&pid, "notify-send", nullptr, nullptr, argv.data(), environ);
  if (rc == ENOENT) {
    return std::unexpected(SendError::Missing);
  }
  if (rc != 0) {
    std::println(stderr, "whether-notify: notify-send: {}", std::strerror(rc));
    return std::unexpected(SendError::Failed);
  }
  int status = 0;
  if (waitpid(pid, &status, 0) < 0 || !WIFEXITED(status) || WEXITSTATUS(status) != 0) {
    return std::unexpected(SendError::Failed);
  }
  return {};
}

std::int64_t unix_now() {
  using namespace std::chrono;
  return duration_cast<seconds>(system_clock::now().time_since_epoch()).count();
}

}  // namespace

int main() {
  wp::Client client;
  notify::Options options;
  notify::Deduper sent;
  bool missing_reported = false;
  return client.run([&](const wp::HostMessage& message) {
    if (const auto* hello = std::get_if<wp::Hello>(&message)) {
      options = notify::Options::from(hello->config);
      client.ready("whether-notify");
    } else if (const auto* state = std::get_if<wp::State>(&message)) {
      for (const auto& n : sent.fresh(notify::decide(*state, options, unix_now()))) {
        const auto result = send(n);
        if (!result && result.error() == SendError::Missing && !missing_reported) {
          client.log(wp::LogLevel::Warn, "notify-send not found; notifications are off");
          missing_reported = true;
        }
      }
    }
  });
}
