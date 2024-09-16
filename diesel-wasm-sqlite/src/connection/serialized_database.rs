use crate::ffi;

/// `SerializedDatabase` is a wrapper for a serialized database that is dynamically allocated by calling `sqlite3_serialize`.
/// This RAII wrapper is necessary to deallocate the memory when it goes out of scope with `sqlite3_free`.
#[derive(Debug)]
pub struct SerializedDatabase {
    pub data: Vec<u8>,
}

impl SerializedDatabase {
    pub(crate) unsafe fn new(data_ptr: *mut u8, len: u32) -> Self {
        let mut data = vec![0; len as usize];
        ffi::raw_copy_from_sqlite(data_ptr, len, data.as_mut_slice());

        crate::get_sqlite_unchecked().sqlite3_free(data_ptr);

        Self { data }
    }
}
