#pragma once
#include <cstdint>
// Deliberately small native ABI fixture. These types never enter JSI.
struct RustBuffer {
  uint64_t capacity;
  uint64_t len;
  uint8_t *data;
};
struct RustCallStatus {
  int8_t code;
  RustBuffer error_buf;
};
extern "C" {
void ffi_xmtp_sdk_rustbuffer_free(RustBuffer, RustCallStatus *);
uint64_t uniffi_xmtp_sdk_fn_func_resume_streams();
uint64_t uniffi_xmtp_sdk_fn_func_suspend_streams();
void ffi_xmtp_sdk_rust_future_poll_void(uint64_t, void (*)(uint64_t, int8_t),
                                        uint64_t);
void ffi_xmtp_sdk_rust_future_complete_void(uint64_t, RustCallStatus *);
void ffi_xmtp_sdk_rust_future_free_void(uint64_t);
uint64_t uniffi_xmtp_sdk_fn_clone_client(uint64_t, RustCallStatus *);
void uniffi_xmtp_sdk_fn_free_client(uint64_t, RustCallStatus *);
uint64_t uniffi_xmtp_sdk_fn_func_sdk_discard_unreturned_client(uint64_t);
}
