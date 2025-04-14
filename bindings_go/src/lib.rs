use libc::{c_int, c_uchar, size_t};
use std::slice;

#[unsafe(no_mangle)]
pub extern "C" fn validate_inbox_id_key_package_ffi(data_ptr: *const c_uchar, data_len: size_t) -> c_int {
    // Safety: Check for null and valid length
    if data_ptr.is_null() || data_len == 0 {
        return -1;
    }

    // Convert raw pointer and length to Vec<u8>
    let data = unsafe {
        slice::from_raw_parts(data_ptr, data_len).to_vec()
    };

    // Call the actual Rust function
    match mls_validation_service::handlers::validate_inbox_id_key_package_ffi(data) {
        true => 0,
        false => -1,
    }
}