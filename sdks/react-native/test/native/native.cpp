#include "XmtpNative.h"
#include "generated/xmtp_sdk_ffi.hpp"
#include <chrono>
#include <condition_variable>
#include <cstdio>
#include <cstdlib>
#include <map>
#include <mutex>
#include <stdexcept>
#include <thread>

namespace {
std::mutex mutex;
std::condition_variable changed;
struct Future {
  int polls = 0;
  bool pending = false;
  void (*wake)(uint64_t, int8_t) = nullptr;
  uint64_t data = 0;
  int lifecycle = -1;
};
std::map<uint64_t, Future> futures;
std::map<uint64_t, int> clones, released, closed;
uint64_t nextFuture = 1;
int freed = 0;
int completed = 0;
bool foreground = true;
bool deferLifecycle = false;
void require(bool pass, const char *message) {
  if (!pass) {
    std::fprintf(stderr, "%s\n", message);
    std::abort();
  }
}
template <class Predicate> void wait(Predicate condition) {
  std::unique_lock lock(mutex);
  require(changed.wait_for(lock, std::chrono::seconds(5), condition),
          "native cleanup did not finish");
}
uint64_t create(bool pending, int lifecycle = -1) {
  std::lock_guard lock(mutex);
  const auto handle = nextFuture++;
  futures.emplace(handle, Future{0, pending, nullptr, 0, lifecycle});
  return handle;
}
} // namespace
extern "C" {
void ffi_xmtp_sdk_rustbuffer_free(RustBuffer, RustCallStatus *) {}
uint64_t uniffi_xmtp_sdk_fn_func_resume_streams() {
  return create(deferLifecycle, 1);
}
uint64_t uniffi_xmtp_sdk_fn_func_suspend_streams() {
  return create(deferLifecycle, 0);
}
void ffi_xmtp_sdk_rust_future_poll_void(uint64_t handle,
                                        void (*wake)(uint64_t, int8_t),
                                        uint64_t data) {
  std::unique_lock lock(mutex);
  auto &future = futures.at(handle);
  ++future.polls;
  // The process latch changes on the first native poll, never creation.
  if (future.lifecycle >= 0)
    foreground = future.lifecycle == 1;
  if (future.pending && future.polls > 1) {
    future.wake = wake;
    future.data = data;
    changed.notify_all();
    return;
  }
  const auto result = future.pending ? 1 : 0;
  lock.unlock();
  wake(data, result);
}
void ffi_xmtp_sdk_rust_future_complete_void(uint64_t, RustCallStatus *) {
  std::lock_guard lock(mutex);
  ++completed;
}
void ffi_xmtp_sdk_rust_future_free_void(uint64_t handle) {
  std::lock_guard lock(mutex);
  require(futures.erase(handle) == 1, "native future freed twice");
  ++freed;
  changed.notify_all();
}
uint64_t uniffi_xmtp_sdk_fn_clone_client(uint64_t pointer, RustCallStatus *) {
  std::lock_guard lock(mutex);
  ++clones[pointer];
  return pointer;
}
void uniffi_xmtp_sdk_fn_free_client(uint64_t pointer, RustCallStatus *) {
  std::lock_guard lock(mutex);
  ++released[pointer];
  changed.notify_all();
}
uint64_t
uniffi_xmtp_sdk_fn_func_sdk_discard_unreturned_client(uint64_t pointer) {
  {
    std::lock_guard lock(mutex);
    ++closed[pointer];
  }
  changed.notify_all();
  return create(false);
}
}
int main() {
  using namespace xmtp::reactnative;
  deferLifecycle = true;
  require(seedLifecycle(false), "background lifecycle seed failed");
  {
    std::lock_guard lock(mutex);
    require(!foreground, "background seed did not synchronously poll suspend");
    require(completed == 0,
            "lifecycle seed waited for pending acknowledgement");
  }
  wait([] { return futures.at(1).wake != nullptr; });
  void (*wake)(uint64_t, int8_t);
  uint64_t data;
  {
    std::lock_guard lock(mutex);
    wake = futures.at(1).wake;
    data = futures.at(1).data;
  }
  wake(data, 0);
  wait([] { return freed == 1; });
  deferLifecycle = false;
  require(seedLifecycle(true), "foreground lifecycle seed failed");
  {
    std::lock_guard lock(mutex);
    require(foreground, "foreground seed did not poll resume");
  }
  {
    ClientOwners owner;
    owner.started(11);
    owner.completed(11, 101, 0);
    // There is deliberately no public track call: JS has not lifted the result.
    owner.stop();
    owner.stop();
  }
  wait([] { return closed[101] == 1; });
  {
    std::lock_guard lock(mutex);
    require(clones[101] == 1,
            "completed Client was not retained before JS lifting");
  }
  {
    ClientOwners owner;
    owner.started(12);
    owner.stop();
    bool rejected = false;
    try {
      owner.completed(12, 102, 0);
    } catch (const std::runtime_error &) {
      rejected = true;
    }
    require(rejected,
            "late native Client result was delivered after invalidation");
  }
  wait([] { return closed[102] == 1; });
  {
    std::lock_guard lock(mutex);
    require(clones[102] == 0,
            "late native result cloned instead of consuming original Arc");
  }
  {
    ClientOwners owner;
    owner.started(13);
    owner.completed(13, 103, 0);
    owner.track(103);
    owner.untrack(103); // Successful public end releases the retained Arc only.
    owner.stop();
    owner.started(14);
    owner.freed(14);
    owner.completed(14, 104,
                    0); // Reused non-Client future must not inherit a tag.
    owner.started(15);
    owner.completed(15, 105, 1);
  }
  {
    std::lock_guard lock(mutex);
    require(clones[103] == 1 && released[103] == 1 && closed[103] == 0,
            "public end did not release exactly one retained Arc");
    require(clones[104] == 0 && closed[104] == 0,
            "freed future retained a stale Client tag");
    require(clones[105] == 0 && closed[105] == 0,
            "failed future treated error as Client");
  }
  wait([] { return freed == 4; });
  require(completed == 4, "native async cleanup did not complete every future");
  std::puts("native lifecycle and Client ownership checks passed");
}
