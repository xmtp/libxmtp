/// trait indicating this type may be built in a way that can manage network cursors.

/// Api Clients may choose their own strategy for managing cursors.
/// for instance, v3 clients may make assumptions about the centralization of a backend.
/// d14n clients may be more careful or strategic when choosing cursors.
/// etc.
pub trait CursorAwareApi {
    type CursorStore;

    /// set the cursor store for this api
    /// a cursor indicates a position in a backend network topic
    fn set_cursor_store(&self, store: Self::CursorStore);
}
