use super::*;
// Section: wire functions

#[no_mangle]
pub extern "C" fn wire_generate_private_preferences_topic_identifier(
    port_: i64,
    private_key_bytes: *mut wire_uint_8_list,
) {
    wire_generate_private_preferences_topic_identifier_impl(port_, private_key_bytes)
}

#[no_mangle]
pub extern "C" fn wire_user_preferences_encrypt(
    port_: i64,
    public_key: *mut wire_uint_8_list,
    private_key: *mut wire_uint_8_list,
    message: *mut wire_uint_8_list,
) {
    wire_user_preferences_encrypt_impl(port_, public_key, private_key, message)
}

#[no_mangle]
pub extern "C" fn wire_user_preferences_decrypt(
    port_: i64,
    public_key: *mut wire_uint_8_list,
    private_key: *mut wire_uint_8_list,
    encrypted_message: *mut wire_uint_8_list,
) {
    wire_user_preferences_decrypt_impl(port_, public_key, private_key, encrypted_message)
}

// Section: allocate functions

#[no_mangle]
pub extern "C" fn new_uint_8_list_0(len: i32) -> *mut wire_uint_8_list {
    let ans = wire_uint_8_list {
        ptr: support::new_leak_vec_ptr(Default::default(), len),
        len,
    };
    support::new_leak_box_ptr(ans)
}

// Section: related functions

// Section: impl Wire2Api

impl Wire2Api<Vec<u8>> for *mut wire_uint_8_list {
    fn wire2api(self) -> Vec<u8> {
        unsafe {
            let wrap = support::box_from_leak_ptr(self);
            support::vec_from_leak_ptr(wrap.ptr, wrap.len)
        }
    }
}
// Section: wire structs

#[repr(C)]
#[derive(Clone)]
pub struct wire_uint_8_list {
    ptr: *mut u8,
    len: i32,
}

// Section: impl NewWithNullPtr

pub trait NewWithNullPtr {
    fn new_with_null_ptr() -> Self;
}

impl<T> NewWithNullPtr for *mut T {
    fn new_with_null_ptr() -> Self {
        std::ptr::null_mut()
    }
}

// Section: sync execution mode utility

#[no_mangle]
pub extern "C" fn free_WireSyncReturn(ptr: support::WireSyncReturn) {
    unsafe {
        let _ = support::box_from_leak_ptr(ptr);
    };
}
