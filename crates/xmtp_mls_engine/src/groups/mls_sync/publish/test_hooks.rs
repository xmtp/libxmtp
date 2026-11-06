//! Pause points in the publish loop that a test fills in.

pub(crate) type DeferralHook = Box<dyn FnOnce() -> futures::future::BoxFuture<'static, ()> + Send>;
static DEFERRAL_HOOKS: std::sync::Mutex<Vec<(Vec<u8>, DeferralHook)>> =
    std::sync::Mutex::new(Vec::new());

/// Run `hook` once, right after the publish loop of this group defers a
/// message. Other work can land between two intents of one pass.
#[cfg(test)]
pub(crate) fn on_next_deferral(group_id: &[u8], hook: DeferralHook) {
    DEFERRAL_HOOKS
        .lock()
        .expect("deferral hooks")
        .push((group_id.to_vec(), hook));
}

static PREPARE_HOOKS: std::sync::Mutex<Vec<(Vec<u8>, DeferralHook)>> =
    std::sync::Mutex::new(Vec::new());

/// Run `hook` once, after the publish loop of this group captures an intent
/// and before it saves the attempt under the writer.
#[cfg(test)]
pub(crate) fn on_next_prepare(group_id: &[u8], hook: DeferralHook) {
    PREPARE_HOOKS
        .lock()
        .expect("prepare hooks")
        .push((group_id.to_vec(), hook));
}

pub(super) async fn before_prepare(group_id: &[u8]) {
    let hook = {
        let mut hooks = PREPARE_HOOKS.lock().expect("prepare hooks");
        hooks
            .iter()
            .position(|(id, _)| id == group_id)
            .map(|index| hooks.remove(index).1)
    };
    if let Some(hook) = hook {
        hook().await;
    }
}

pub(super) async fn after_deferral(group_id: &[u8]) {
    let hook = {
        let mut hooks = DEFERRAL_HOOKS.lock().expect("deferral hooks");
        hooks
            .iter()
            .position(|(id, _)| id == group_id)
            .map(|index| hooks.remove(index).1)
    };
    if let Some(hook) = hook {
        hook().await;
    }
}
