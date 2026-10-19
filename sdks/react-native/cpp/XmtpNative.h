#pragma once

#include <cstdint>
#include <mutex>
#include <unordered_map>
#include <unordered_set>

namespace xmtp::reactnative {
// Own Client results across the gap between native completion and JS lifting.
// All operations are native and safe during React runtime invalidation.
class ClientOwners final {
public:
  ~ClientOwners();
  void started(uint64_t future);
  void freed(uint64_t future);
  void completed(uint64_t future, uint64_t pointer, int8_t status);
  void track(uint64_t pointer);
  void untrack(uint64_t pointer);
  void stop();

private:
  std::mutex mutex_;
  bool stopped_ = false;
  std::unordered_map<uint64_t, uint64_t> clients_;
  std::unordered_set<uint64_t> futures_;
};

// These functions use only native UniFFI futures. They never enter JavaScript.
bool seedLifecycle(bool foreground);
void setForeground(bool foreground);
uint64_t retainClient(uint64_t pointer);
void releaseClient(uint64_t pointer);
void closeClient(uint64_t pointer);
} // namespace xmtp::reactnative
