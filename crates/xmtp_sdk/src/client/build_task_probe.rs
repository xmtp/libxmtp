use super::*;

#[derive(Default)]
pub(crate) struct BuildTaskProbe {
    pub(crate) started: tokio::sync::Notify,
    pub(crate) task: parking_lot::Mutex<Option<tokio::task::AbortHandle>>,
    pub(crate) client: parking_lot::Mutex<Option<Arc<CoreClient>>>,
}

tokio::task_local! {
    pub(crate) static CURRENT: Arc<BuildTaskProbe>;
}
