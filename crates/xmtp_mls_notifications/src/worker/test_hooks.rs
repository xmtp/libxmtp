//! A local change that a test runs just before the task sends a request.

thread_local! {
    static BEFORE_REQUEST: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const {
        std::cell::RefCell::new(None)
    };
}

#[cfg(test)]
pub(crate) struct BeforeRequestGuard(std::marker::PhantomData<std::rc::Rc<()>>);

#[cfg(test)]
impl Drop for BeforeRequestGuard {
    fn drop(&mut self) {
        BEFORE_REQUEST.with(|hook| hook.borrow_mut().take());
    }
}

/// Run one local change before a request is sent: after an update batch is
/// prepared, or before a registration takes the request lock.
#[cfg(test)]
pub(crate) fn on_before_request(change: impl FnOnce() + 'static) -> BeforeRequestGuard {
    BEFORE_REQUEST.with(|hook| {
        assert!(hook.borrow_mut().replace(Box::new(change)).is_none());
    });
    BeforeRequestGuard(Default::default())
}

pub(crate) fn before_request() {
    let change = BEFORE_REQUEST.with(|hook| hook.borrow_mut().take());
    if let Some(change) = change {
        change();
    }
}
