#include "XmtpNative.h"
#include "generated/xmtp_sdk_ffi.hpp"

#include <condition_variable>
#include <cstdio>
#include <memory>
#include <mutex>
#include <optional>
#include <stdexcept>
#include <thread>

namespace xmtp::reactnative {
namespace {
constexpr int8_t kFutureReady = 0;

void releaseError(RustCallStatus &status) {
  if (status.error_buf.data != nullptr) {
    RustCallStatus ignored{};
    ffi_xmtp_sdk_rustbuffer_free(status.error_buf, &ignored);
    status.error_buf = {};
  }
}

// One outstanding UniFFI poll owns this state until its native continuation
// returns. Tokio wakes the native waiter; this class never touches JSI.
class NativeFuture {
public:
  explicit NativeFuture(uint64_t handle) : handle_(handle) { poll(); }
  ~NativeFuture() { ffi_xmtp_sdk_rust_future_free_void(handle_); }

  bool ready() {
    std::lock_guard lock(mutex_);
    return result_ == kFutureReady;
  }

  bool finish() {
    for (;;) {
      std::unique_lock lock(mutex_);
      changed_.wait(lock, [&] { return result_.has_value(); });
      if (*result_ == kFutureReady)
        break;
      lock.unlock();
      poll();
    }
    RustCallStatus status{};
    ffi_xmtp_sdk_rust_future_complete_void(handle_, &status);
    const bool success = status.code == 0;
    releaseError(status);
    return success;
  }

private:
  void poll() {
    {
      std::lock_guard lock(mutex_);
      result_.reset();
    }
    ffi_xmtp_sdk_rust_future_poll_void(
        handle_,
        [](uint64_t data, int8_t result) {
          auto &state = *reinterpret_cast<NativeFuture *>(data);
          std::lock_guard lock(state.mutex_);
          state.result_ = result;
          state.changed_.notify_one();
        },
        reinterpret_cast<uint64_t>(this));
  }
  uint64_t handle_;
  std::mutex mutex_;
  std::condition_variable changed_;
  std::optional<int8_t> result_;
};

std::mutex lifecycleMutex;
} // namespace

bool seedLifecycle(bool foreground) {
  // The first native poll sets the Rust process latch and queues commands
  // before its first await. Serialize that poll with other OS transitions.
  // Waiting for network acknowledgements must not block the OS main thread.
  std::lock_guard transition(lifecycleMutex);
  auto future = std::make_shared<NativeFuture>(
      foreground ? uniffi_xmtp_sdk_fn_func_resume_streams()
                 : uniffi_xmtp_sdk_fn_func_suspend_streams());
  if (future->ready())
    return future->finish();
  std::thread([future] {
    if (!future->finish()) {
      std::fputs("XMTP native lifecycle transition failed\n", stderr);
    }
  }).detach();
  return true;
}

void setForeground(bool foreground) {
  if (!seedLifecycle(foreground)) {
    std::fputs("XMTP native lifecycle transition failed\n", stderr);
  }
}

uint64_t retainClient(uint64_t pointer) {
  RustCallStatus status{};
  const auto clone = uniffi_xmtp_sdk_fn_clone_client(pointer, &status);
  const bool success = status.code == 0;
  releaseError(status);
  return success ? clone : 0;
}

void releaseClient(uint64_t pointer) {
  RustCallStatus status{};
  uniffi_xmtp_sdk_fn_free_client(pointer, &status);
  releaseError(status);
}

void closeClient(uint64_t pointer) {
  std::thread([pointer] {
    // This internal SDK primitive closes listeners and storage and consumes
    // the retained Arc. It also disconnects storage if ordinary end fails.
    NativeFuture future(
        uniffi_xmtp_sdk_fn_func_sdk_discard_unreturned_client(pointer));
    if (!future.finish()) {
      std::fputs("XMTP native runtime client cleanup failed\n", stderr);
    }
  }).detach();
}
void ClientOwners::started(uint64_t future) {
  std::lock_guard lock(mutex_);
  futures_.insert(future);
}

void ClientOwners::freed(uint64_t future) {
  std::lock_guard lock(mutex_);
  futures_.erase(future);
}

void ClientOwners::completed(uint64_t future, uint64_t pointer, int8_t status) {
  std::unique_lock lock(mutex_);
  if (futures_.erase(future) == 0 || status != 0)
    return;
  if (clients_.find(pointer) != clients_.end())
    return;
  if (!stopped_) {
    const auto clone = retainClient(pointer);
    if (clone != 0) {
      clients_.emplace(pointer, clone);
      return;
    }
  }
  lock.unlock();
  // complete_u64 transferred the original Arc out of the Rust future. No JS
  // wrapper owns it yet, so consume it directly when delivery is impossible.
  closeClient(pointer);
  throw std::runtime_error("XMTP runtime cannot accept a completed Client");
}

void ClientOwners::track(uint64_t pointer) {
  std::lock_guard lock(mutex_);
  if (stopped_)
    throw std::runtime_error("XMTP runtime was invalidated");
  // Native completion retains the Client before JS can adopt it. A JS handle
  // can confirm that ownership, but cannot authorize a raw UniFFI clone.
  if (clients_.find(pointer) == clients_.end())
    throw std::runtime_error("XMTP Client was not retained by native completion");
}

void ClientOwners::untrack(uint64_t pointer) {
  uint64_t clone = 0;
  {
    std::lock_guard lock(mutex_);
    const auto found = clients_.find(pointer);
    if (found == clients_.end())
      return;
    clone = found->second;
    clients_.erase(found);
  }
  releaseClient(clone);
}

void ClientOwners::stop() {
  std::unordered_map<uint64_t, uint64_t> clients;
  {
    std::lock_guard lock(mutex_);
    if (stopped_)
      return;
    stopped_ = true;
    clients.swap(clients_);
  }
  for (const auto &entry : clients)
    closeClient(entry.second);
}

ClientOwners::~ClientOwners() { stop(); }
} // namespace xmtp::reactnative
