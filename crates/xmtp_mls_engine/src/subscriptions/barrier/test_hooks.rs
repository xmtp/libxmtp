//! Points in Welcome discovery and the barrier loop that a test observes or
//! fills in.

std::thread_local! {
    static BEFORE_PROGRESS_READ: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        const { std::cell::RefCell::new(None) };
    /// Groups-table reads by Welcome discovery on this thread.
    pub(crate) static GROUPS_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    pub(crate) static AFTER_DISCOVERY_READ: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        const { std::cell::RefCell::new(None) };
}

pub(super) fn count_groups_read() {
    GROUPS_READS.with(|reads| reads.set(reads.get() + 1));
}

/// Runs between Welcome discovery's reads and its gate record.
pub(super) fn after_discovery_read() {
    let hook = AFTER_DISCOVERY_READ.with(|slot| slot.borrow_mut().take());
    if let Some(hook) = hook {
        hook();
    }
}

pub(super) fn before_progress_read() {
    let hook = BEFORE_PROGRESS_READ.with(|slot| slot.borrow_mut().take());
    if let Some(hook) = hook {
        hook();
    }
}

#[cfg(test)]
pub(crate) struct ProgressReadHook(std::marker::PhantomData<std::rc::Rc<()>>);

#[cfg(test)]
impl Drop for ProgressReadHook {
    fn drop(&mut self) {
        BEFORE_PROGRESS_READ.with(|slot| slot.borrow_mut().take());
    }
}

#[cfg(test)]
pub(crate) fn on_next_progress_read(hook: impl FnOnce() + 'static) -> ProgressReadHook {
    assert!(
        BEFORE_PROGRESS_READ
            .with(|slot| slot.replace(Some(Box::new(hook))))
            .is_none()
    );
    ProgressReadHook(std::marker::PhantomData)
}
