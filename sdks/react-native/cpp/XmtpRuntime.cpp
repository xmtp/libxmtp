#include "XmtpRuntime.h"
#include "XmtpNative.h"
#include "generated/xmtp_sdk.hpp"
#include <stdexcept>
#include <unordered_map>

namespace xmtp::reactnative {
namespace jsi = facebook::jsi;
namespace {
std::mutex ownersMutex;
std::unordered_map<uniffi_runtime::UniffiCallInvoker *, std::weak_ptr<Runtime>>
    owners;
} // namespace

std::shared_ptr<Runtime>
Runtime::install(jsi::Runtime &runtime,
                 std::shared_ptr<facebook::react::CallInvoker> invoker) {
  auto owner = std::shared_ptr<Runtime>(new Runtime());
  owner->runtime_ = &runtime;
  owner->invoker_ = NativeXmtpSdk::registerModule(runtime, std::move(invoker));
  {
    std::lock_guard lock(ownersMutex);
    owners.emplace(owner->invoker_.get(), owner);
  }
  jsi::Object host(runtime);
  host.setProperty(runtime, "track",
                   jsi::Function::createFromHostFunction(
                       runtime, jsi::PropNameID::forAscii(runtime, "track"), 1,
                       [owner](jsi::Runtime &rt, const jsi::Value &,
                               const jsi::Value *args, size_t count) {
                         if (count != 1 || !args[0].isBigInt()) {
                           throw jsi::JSError(
                               rt,
                               "XMTP client owner requires a bigint handle");
                         }
                         owner->track(rt, args[0].asBigInt(rt).getUint64(rt));
                         return jsi::Value::undefined();
                       }));
  host.setProperty(
      runtime, "untrack",
      jsi::Function::createFromHostFunction(
          runtime, jsi::PropNameID::forAscii(runtime, "untrack"), 1,
          [owner](jsi::Runtime &rt, const jsi::Value &, const jsi::Value *args,
                  size_t count) {
            if (count != 1 || !args[0].isBigInt()) {
              throw jsi::JSError(rt,
                                 "XMTP client owner requires a bigint handle");
            }
            owner->untrack(args[0].asBigInt(rt).getUint64(rt));
            return jsi::Value::undefined();
          }));
  runtime.global().setProperty(runtime, "__xmtpNativeOwners", std::move(host));
  return owner;
}

std::shared_ptr<Runtime> Runtime::find(
    const std::shared_ptr<uniffi_runtime::UniffiCallInvoker> &generation) {
  std::lock_guard lock(ownersMutex);
  const auto found = owners.find(generation.get());
  return found == owners.end() ? nullptr : found->second.lock();
}

void Runtime::clientFutureStarted(
    const std::shared_ptr<uniffi_runtime::UniffiCallInvoker> &generation,
    uint64_t future) {
  auto owner = find(generation);
  if (!owner)
    throw std::runtime_error("XMTP native Client owner is unavailable");
  owner->clients_.started(future);
}

void Runtime::clientFutureFreed(
    const std::shared_ptr<uniffi_runtime::UniffiCallInvoker> &generation,
    uint64_t future) {
  if (auto owner = find(generation)) {
    owner->clients_.freed(future);
  }
}

void Runtime::clientFutureCompleted(
    const std::shared_ptr<uniffi_runtime::UniffiCallInvoker> &generation,
    uint64_t future, uint64_t pointer, int8_t status) {
  auto owner = find(generation);
  if (!owner)
    return; // No JS wrapper can run after its host owner is destroyed.
  owner->clients_.completed(future, pointer, status);
}

void Runtime::track(jsi::Runtime &runtime, uint64_t pointer) {
  try {
    clients_.track(pointer);
  } catch (const std::exception &) {
    throw jsi::JSError(runtime, "XMTP native runtime cannot retain Client");
  }
}

void Runtime::untrack(uint64_t pointer) { clients_.untrack(pointer); }

void Runtime::stop() {
  {
    std::lock_guard lock(mutex_);
    if (stopped_)
      return;
    stopped_ = true;
    if (invoker_)
      invoker_->stop();
  }
  clients_.stop();
}

void Runtime::invalidate() {
  stop();
  if (runtime_) {
    NativeXmtpSdk::unregisterModule(*runtime_, invoker_);
    runtime_ = nullptr;
  }
}

Runtime::~Runtime() {
  stop();
  std::lock_guard lock(ownersMutex);
  owners.erase(invoker_.get());
}
} // namespace xmtp::reactnative
