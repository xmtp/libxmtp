use super::*;

// Native calls run on an owned task in every profile. This keeps SQLite work
// off the JavaScript thread, gives nested MLS work a fresh executor stack, and
// lets work finish if the FFI call is cancelled. On wasm32, cancellation drops
// the work because the target has no blocking thread pool.
// Swift's cooperative threads have small stacks. Callers box large work
// futures before this helper, and this helper boxes the spawn future.
#[cfg(not(target_arch = "wasm32"))]
const MAX_SDK_WORKER_FUTURE_BYTES: usize = 2 * 1024;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn on_sdk_worker<T, F>(context: MlsContext, work: F) -> Result<T, XmtpError>
where
    T: Send + 'static,
    F: Future<Output = Result<T, XmtpError>> + Send + 'static,
{
    const { assert!(std::mem::size_of::<F>() <= MAX_SDK_WORKER_FUTURE_BYTES) };
    xmtp_common::spawn(None, Box::pin(while_open(context, work)))
        .join()
        .await
        .map_err(XmtpError::from_core)?
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn on_sdk_worker<T, F>(context: MlsContext, work: F) -> Result<T, XmtpError>
where
    F: Future<Output = Result<T, XmtpError>>,
{
    while_open(context, work).await
}

/// Enter the call gate, or fail with `ClientClosed` after end() has begun.
/// Hold the guard until the call stops using the database, so end() does not
/// disconnect the database under it. A call that holds the guard must not wait
/// for end() of its own client: end() waits for that call.
pub(crate) fn enter_call(context: &MlsContext) -> Result<ForegroundCall, XmtpError> {
    let call = context
        .foreground_calls()
        .enter()
        .ok_or_else(XmtpError::closed)?;
    if context.is_closed() {
        return Err(XmtpError::closed());
    }
    Ok(call)
}

// Check the closed state in the task because end() can run before it starts.
async fn while_open<T, F>(context: MlsContext, work: F) -> Result<T, XmtpError>
where
    F: Future<Output = Result<T, XmtpError>>,
{
    let _call = enter_call(&context)?;
    work.await.map_err(|error| {
        if context.is_closed() {
            XmtpError::closed()
        } else {
            error
        }
    })
}

pub(super) fn deletion_group(
    group: MlsGroup<MlsContext>,
    stored: &StoredGroupMessage,
) -> Result<MlsGroup<MlsContext>, XmtpError> {
    if group.conversation_type == ConversationType::Dm
        && stored.sender_inbox_id != group.context.inbox_id()
    {
        return Err(XmtpError::conversation_permission_denied(
            "not your message",
        ));
    }
    if stored.group_id == group.group_id {
        return Ok(group);
    }
    let stitched = group
        .context
        .db()
        .fetch_stitched(&stored.group_id)
        .map_err(XmtpError::from_core)?;
    if stitched.is_none_or(|winner| winner.id != group.group_id) {
        return Err(XmtpError::conversation_permission_denied(
            "message belongs to another conversation",
        ));
    }
    MlsStore::new(group.context.clone())
        .group(&stored.group_id)
        .map_err(XmtpError::from_core)
}
