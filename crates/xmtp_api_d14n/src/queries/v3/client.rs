/// Client shell retained for the bidi binding until that lane is merged.
#[derive(Clone)]
pub struct V3Client<C, Store> {
    pub(super) client: C,
    pub cursor_store: Store,
}

impl<C, Store> V3Client<C, Store> {
    pub fn new(client: C, cursor_store: Store) -> Self {
        Self {
            client,
            cursor_store,
        }
    }
}
