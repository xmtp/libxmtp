// Actual generated XMTP callbacks, real Rust buffers and real Hermes runtimes.
// This does not replace the installed ReactHost/platform lifecycle journey.
#include <cassert>
#include <deque>
#include <hermes/hermes.h>
#include <iostream>
#ifndef GENERATED_WRAPPER
#error "Pass the generated XMTP C++ wrapper as GENERATED_WRAPPER"
#endif
#include GENERATED_WRAPPER
namespace sdk = uniffi::xmtp_sdk;
static int completed = 0;
static void checkStatus(RustCallStatus status) {
  assert(status.code == 2);
  assert(status.error_buf.len != 0);
  sdk::Bridging<RustBuffer>::rustbuffer_free(status.error_buf);
  ++completed;
}
static void completeVoid(uint64_t data, UniffiForeignFutureResultVoid result) {
  assert(data == 42);
  checkStatus(result.call_status);
}
static void completeBuffer(uint64_t data,
                           UniffiForeignFutureResultRustBuffer result) {
  assert(data == 42);
  assert(result.return_value.data == nullptr && result.return_value.len == 0);
  checkStatus(result.call_status);
}
class Queue : public facebook::react::CallInvoker {
public:
  std::deque<facebook::react::CallFunc> tasks;
  void invokeAsync(facebook::react::CallFunc &&f) noexcept override {
    tasks.push_back(std::move(f));
  }
  void invokeSync(facebook::react::CallFunc &&) override { std::abort(); }
  void drain(facebook::jsi::Runtime &rt) {
    while (!tasks.empty()) {
      auto f = std::move(tasks.front());
      tasks.pop_front();
      f(rt);
    }
  }
};
static void eval(facebook::jsi::Runtime &rt, const std::string &source) {
  rt.evaluateJavaScript(std::make_shared<facebook::jsi::StringBuffer>(source),
                        "callback-test.js");
}
static int successes = 0;
static void completeLive(uint64_t data, UniffiForeignFutureResultVoid result) {
  assert(data == 42);
  if (result.call_status.code == 0)
    ++successes;
  else
    checkStatus(result.call_status);
}
static auto install(facebook::jsi::Runtime &rt, std::shared_ptr<Queue> queue) {
  auto ci = std::make_shared<uniffi_runtime::UniffiCallInvoker>(queue);
  auto generation = uniffi_runtime::installHandleAllocator(rt, ci);
  eval(rt, R"JS(
    globalThis.calls = 0;
    globalThis.cb = (handle, complete, data) => {
      calls++;
      globalThis.finish = () => complete(data, {call_status: {code: 0}});
      globalThis.completeRaw = complete; globalThis.completeData = data;
      return {handle: __ubrnHandleAllocator.allocate('foreign-future-drop'),
              free: token => __ubrnHandleAllocator.release(token)};
    };
  )JS");
  sdk::cb::callbackinterfacepreauthenticatemethod0::
      vtablecallbackinterfacepreauthenticate::makeCallbackFunction(
          rt, ci, rt.global().getProperty(rt, "cb"));
  return generation;
}
static void liveReload() {
  auto &registry = uniffi_runtime::HandleRegistry::instance();
  auto rt = facebook::hermes::makeHermesRuntime();
  auto queue = std::make_shared<Queue>();
  auto first = install(*rt, queue);
  auto old = registry.allocate(first, "callback");
  UniffiForeignFutureDroppedCallbackStruct dropped{};
  sdk::cb::callbackinterfacepreauthenticatemethod0::
      vtablecallbackinterfacepreauthenticate::callback(old, completeLive, 42,
                                                       &dropped);
  assert(rt->global().getProperty(*rt, "calls").asNumber() == 1);
  assert(first->pending.size() == 1);
  int before = completed;
  first->invalidate();
  assert(completed == before + 1 && first->pending.empty());
  eval(*rt, "finish()"); // Completion after teardown must not consume the
                         // sender again.
  assert(completed == before + 1);
  rt.reset();
  first.reset();
  auto replacement = facebook::hermes::makeHermesRuntime();
  auto second = install(*replacement, queue);
  auto current = registry.allocate(second, "callback");
  assert(current != old);
  UniffiForeignFutureDroppedCallbackStruct ignored{};
  sdk::cb::callbackinterfacepreauthenticatemethod0::
      vtablecallbackinterfacepreauthenticate::callback(old, completeLive, 42,
                                                       &ignored);
  assert(completed == before + 2 &&
         replacement->global().getProperty(*replacement, "calls").asNumber() ==
             0);
  dropped.free(dropped.handle);
  sdk::st::vtablecallbackinterfacepreauthenticate::
      vtablecallbackinterfacepreauthenticate::free::callback(old);
  queue->drain(*replacement);
  assert(replacement->global().getProperty(*replacement, "calls").asNumber() ==
         0);
  sdk::cb::callbackinterfacepreauthenticatemethod0::
      vtablecallbackinterfacepreauthenticate::callback(current, completeLive,
                                                       42, &dropped);
  assert(replacement->global().getProperty(*replacement, "calls").asNumber() ==
         1);
  eval(*replacement, "finish(); finish()");
  assert(successes == 1 && second->pending.empty());
  dropped.free(dropped.handle);
  queue->drain(*replacement);
  second->invalidate();
  registry.release(current);
}
static void lateBuffersAreDiscarded() {
  auto rt = facebook::hermes::makeHermesRuntime();
  auto queue = std::make_shared<Queue>();
  auto generation = install(*rt, queue);
  auto handle = uniffi_runtime::HandleRegistry::instance().allocate(generation,
                                                                    "callback");
  sdk::cb::callbackinterfacesignermethod0::vtablecallbackinterfacesigner::
      makeCallbackFunction(*rt, generation->invoker,
                           rt->global().getProperty(*rt, "cb"));
  UniffiForeignFutureDroppedCallbackStruct dropped{};
  sdk::cb::callbackinterfacesignermethod0::vtablecallbackinterfacesigner::
      callback(handle, completeBuffer, 42, &dropped);
  generation->invalidate();
  auto ownedView = [&](const char *name) {
    auto native = sdk::Bridging<RustBuffer>::rustbuffer_alloc(12);
    auto value =
        sdk::Bridging<RustBuffer>::toJs(*rt, generation->invoker, native);
    value.asObject(*rt).setProperty(*rt, uniffi_jsi::kUbrnRustCapacity,
                                    static_cast<double>(native.capacity));
    rt->global().setProperty(*rt, name, std::move(value));
  };
  ownedView("lateReturn");
  ownedView("lateError");
  eval(*rt, "completeRaw(completeData, {return_value: lateReturn, call_status: "
            "{code: 2, errorBuf: lateError}})");
  assert(rt->global()
             .getProperty(*rt, "lateReturn")
             .asObject(*rt)
             .getProperty(*rt, uniffi_jsi::kUbrnRustCapacity)
             .asNumber() == 0);
  assert(rt->global()
             .getProperty(*rt, "lateError")
             .asObject(*rt)
             .getProperty(*rt, uniffi_jsi::kUbrnRustCapacity)
             .asNumber() == 0);
  // Repeated completion with consumed views must not free either allocation
  // twice.
  eval(*rt, "completeRaw(completeData, {return_value: lateReturn, call_status: "
            "{code: 2, errorBuf: lateError}})");
  dropped.free(dropped.handle);
  uniffi_runtime::HandleRegistry::instance().release(handle);
}
int main() {
  auto &registry = uniffi_runtime::HandleRegistry::instance();
  auto token = registry.allocate(nullptr, "callback");
  assert(token & 1);
  auto clone = sdk::cb::callbackinterfaceclone::
      vtablecallbackinterfacepreauthenticate::callback(token);
  assert(clone == token && clone != 0);
  UniffiForeignFutureDroppedCallbackStruct dropped{};
  sdk::cb::callbackinterfacepreauthenticatemethod0::
      vtablecallbackinterfacepreauthenticate::callback(token, completeVoid, 42,
                                                       &dropped);
  assert(completed == 1 && dropped.free != nullptr);
  dropped.free(dropped.handle);
  auto input = sdk::Bridging<RustBuffer>::rustbuffer_alloc(32);
  sdk::cb::callbackinterfaceeventlistenermethod0::
      vtablecallbackinterfaceeventlistener::callback(token, input, completeVoid,
                                                     42, &dropped);
  assert(completed == 2 && dropped.free != nullptr);
  dropped.free(dropped.handle);
  auto request = sdk::Bridging<RustBuffer>::rustbuffer_alloc(16);
  sdk::cb::callbackinterfacesignermethod2::vtablecallbackinterfacesigner::
      callback(token, request, completeBuffer, 42, &dropped);
  assert(completed == 3 && dropped.free != nullptr);
  dropped.free(dropped.handle);
  sdk::st::vtablecallbackinterfacepreauthenticate::
      vtablecallbackinterfacepreauthenticate::free::callback(token);
  assert(registry.retain(token) ==
         token); // The stale clone still owns one ref.
  registry.release(token);
  sdk::st::vtablecallbackinterfacepreauthenticate::
      vtablecallbackinterfacepreauthenticate::free::callback(token);
  assert(registry.retain(token) == 0);
  struct Table {
    int marker;
  };
  auto first = sdk::registry::putTable("fixture", Table{7});
  auto second = sdk::registry::putTable("fixture", Table{9});
  assert(first == second && second->marker == 7);
  liveReload();
  lateBuffersAreDiscarded();
  std::cout << "generated XMTP stale callback ABI passed\n";
}
