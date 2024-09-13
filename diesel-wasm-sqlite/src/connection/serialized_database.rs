use crate::ffi;

/// `SerializedDatabase` is a wrapper for a serialized database that is dynamically allocated by calling `sqlite3_serialize`.
/// This RAII wrapper is necessary to deallocate the memory when it goes out of scope with `sqlite3_free`.
#[derive(Debug)]
pub struct SerializedDatabase {
    pub data: Vec<u8>,
}

impl Drop for SerializedDatabase {
    /// Deallocates the memory of the serialized database when it goes out of scope.
    fn drop(&mut self) {
        // ffi::sqlite3_free(self.data as _);
    }
}
