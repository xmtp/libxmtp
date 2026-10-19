#pragma once

#include "XmtpNative.h"
#include <ReactCommon/CallInvoker.h>
#include <jsi/jsi.h>
#include <memory>
#include <mutex>

namespace uniffi_runtime {
class UniffiCallInvoker;
}

namespace xmtp::reactnative {
class Runtime final : public std::enable_shared_from_this<Runtime> {
public:
  static std::shared_ptr<Runtime>
  install(facebook::jsi::Runtime &runtime,
          std::shared_ptr<facebook::react::CallInvoker> invoker);
  // Safe on the native module invalidation thread. This releases all SDK
  // ownership without waiting for JS callbacks or entering the dying runtime.
  void stop();
  // Must run synchronously on the JS thread before its engine is destroyed.
  // Platform before-destroy hooks call this while the runtime is still live.
  void invalidate();
  ~Runtime();

  // Generated native wrappers call these before exposing a completed Client
  // to JSI. The generation token prevents handle reuse across runtime reloads.
  static void clientFutureStarted(
      const std::shared_ptr<uniffi_runtime::UniffiCallInvoker> &generation,
      uint64_t future);
  static void clientFutureCompleted(
      const std::shared_ptr<uniffi_runtime::UniffiCallInvoker> &generation,
      uint64_t future, uint64_t pointer, int8_t status);
  static void clientFutureFreed(
      const std::shared_ptr<uniffi_runtime::UniffiCallInvoker> &generation,
      uint64_t future);

private:
  Runtime() = default;
  static std::shared_ptr<Runtime>
  find(const std::shared_ptr<uniffi_runtime::UniffiCallInvoker> &generation);
  void track(facebook::jsi::Runtime &runtime, uint64_t pointer);
  void untrack(uint64_t pointer);
  std::mutex mutex_;
  bool stopped_ = false;
  ClientOwners clients_;
  std::shared_ptr<uniffi_runtime::UniffiCallInvoker> invoker_;
  facebook::jsi::Runtime *runtime_ = nullptr;
};
} // namespace xmtp::reactnative
