mod mls_validation;

use std::ffi::CString;
use libc::{c_char, c_uchar, size_t};
use std::slice;
use crate::mls_validation::validate_inbox_id_key_package;


#[repr(C)]
pub struct ValidationResult {
    pub ok: bool,
    pub message: *mut c_char,
}


#[unsafe(no_mangle)]
pub extern "C" fn validate_inbox_id_key_package_ffi(data_ptr: *const c_uchar, data_len: size_t) -> ValidationResult {
    // Safety: Check for null and valid length
    if data_ptr.is_null() || data_len == 0 {
        return ValidationResult{ok: false, message: CString::new("Invalid data").unwrap().into_raw()};
    }

    // Convert raw pointer and length to Vec<u8>
    let data = unsafe {
        slice::from_raw_parts(data_ptr, data_len).to_vec()
    };

    // Call the actual Rust function
    match validate_inbox_id_key_package(data) {
        Ok(_) => ValidationResult{ok: true, message: CString::new("").unwrap().into_raw()},
        Err(err) => ValidationResult {
            ok: false,
            message: CString::new(format!("{err}")).unwrap().into_raw(),
        },
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn free_c_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe { let _ = CString::from_raw(s); };
    }
}