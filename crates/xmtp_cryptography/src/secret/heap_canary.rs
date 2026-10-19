//! Heap-canary test allocator: proves that a secret is wiped before its heap
//! block is freed.
//!
//! A test binary installs the allocator with
//! [`install_heap_canary!`](crate::install_heap_canary), builds the secret and
//! a [`Canary`] for it, then runs the code under test inside [`scan`] or
//! [`scan_async`]. While a scan is armed, every freed block that is at least
//! as long as the shortest armed form is searched for each form of each
//! canary. A block that holds any form counts as one hit for that canary.
//!
//! - Build the secret and its canary before arming. A copy freed before the
//!   scan is not seen, and the canary itself lives on the stack.
//! - Frees on every thread count while armed, so tokio workers are covered.
//! - `realloc` keeps the default `alloc` + copy + `dealloc`, so a block that
//!   grows while it holds the secret is scanned too.
//! - Stack and register copies are invisible to a heap scan.
//! - Arming takes a process-wide lock, so concurrent scans in one process
//!   run one after another.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::UnsafeCell,
    future::Future,
    sync::{
        Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst},
    },
};

/// Most canaries one scan can arm.
pub const MAX_CANARIES: usize = 8;
/// Shortest secret a canary accepts.
pub const MIN_SECRET_LEN: usize = 16;
/// Longest secret a canary accepts.
pub const MAX_SECRET_LEN: usize = 64;

/// Raw bytes, lowercase hex, byte-reversed raw bytes.
const MAX_FORMS: usize = 3;
const MAX_FORM_LEN: usize = MAX_SECRET_LEN * 2;

/// One searchable byte form of a secret, stored inline.
#[derive(Clone, Copy)]
struct Form {
    bytes: [u8; MAX_FORM_LEN],
    len: usize,
}

impl Form {
    const EMPTY: Self = Self {
        bytes: [0; MAX_FORM_LEN],
        len: 0,
    };

    fn raw(secret: &[u8]) -> Self {
        let mut form = Self::EMPTY;
        form.bytes[..secret.len()].copy_from_slice(secret);
        form.len = secret.len();
        form
    }

    fn lower_hex(secret: &[u8]) -> Self {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut form = Self::EMPTY;
        for (i, byte) in secret.iter().enumerate() {
            form.bytes[2 * i] = DIGITS[usize::from(byte >> 4)];
            form.bytes[2 * i + 1] = DIGITS[usize::from(byte & 0x0f)];
        }
        form.len = secret.len() * 2;
        form
    }

    fn reversed(secret: &[u8]) -> Self {
        let mut form = Self::raw(secret);
        form.bytes[..secret.len()].reverse();
        form
    }

    fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

/// The forms of one secret to look for in freed heap blocks.
///
/// Every form is computed on the stack, so building a canary frees nothing
/// that holds the secret.
#[derive(Clone, Copy)]
pub struct Canary {
    forms: [Form; MAX_FORMS],
    count: usize,
}

impl Canary {
    /// Look for the raw bytes and their lowercase hex.
    ///
    /// # Panics
    ///
    /// If `secret` is shorter than [`MIN_SECRET_LEN`] or longer than
    /// [`MAX_SECRET_LEN`] bytes.
    pub fn new(secret: &[u8]) -> Self {
        assert!(
            (MIN_SECRET_LEN..=MAX_SECRET_LEN).contains(&secret.len()),
            "a canary secret is {MIN_SECRET_LEN} to {MAX_SECRET_LEN} bytes, got {}",
            secret.len()
        );
        let mut forms = [Form::EMPTY; MAX_FORMS];
        forms[0] = Form::raw(secret);
        forms[1] = Form::lower_hex(secret);
        Self { forms, count: 2 }
    }

    /// Look for a secp256k1 key as raw bytes, lowercase hex, and byte-reversed
    /// raw bytes. k256 stores its scalar as little-endian limbs, so the key it
    /// holds in memory is the big-endian key reversed.
    pub fn secp256k1(key: &[u8; 32]) -> Self {
        let mut canary = Self::new(key);
        canary.forms[2] = Form::reversed(key);
        canary.count = 3;
        canary
    }

    fn forms(&self) -> &[Form] {
        &self.forms[..self.count]
    }
}

/// Forms armed for the current scan. Written only by the scan that holds
/// [`SCAN_LOCK`], while [`ARMED`] is false and no free is being scanned.
struct ArmedForms(UnsafeCell<ArmedFormsInner>);

struct ArmedFormsInner {
    forms: [Form; MAX_CANARIES * MAX_FORMS],
    /// The canary index of each form.
    owners: [usize; MAX_CANARIES * MAX_FORMS],
    count: usize,
    min_len: usize,
}

// SAFETY: the cell is written only while `ARMED` is false and `IN_FLIGHT` is
// zero, under `SCAN_LOCK`; `dealloc` reads it only while `ARMED` is true.
unsafe impl Sync for ArmedForms {}

static SCAN_LOCK: Mutex<()> = Mutex::new(());
static ARMED: AtomicBool = AtomicBool::new(false);
/// Frees that may be reading [`FORMS`]. Disarming waits for zero.
static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);
static FORMS: ArmedForms = ArmedForms(UnsafeCell::new(ArmedFormsInner {
    forms: [Form::EMPTY; MAX_CANARIES * MAX_FORMS],
    owners: [0; MAX_CANARIES * MAX_FORMS],
    count: 0,
    min_len: usize::MAX,
}));
static HITS: [AtomicUsize; MAX_CANARIES] = [const { AtomicUsize::new(0) }; MAX_CANARIES];

/// The heap-canary global allocator. Install it with
/// [`install_heap_canary!`](crate::install_heap_canary).
pub struct Scanner;

// SAFETY: every allocation is delegated to `System`; `dealloc` only reads the
// block before handing it back.
unsafe impl GlobalAlloc for Scanner {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        IN_FLIGHT.fetch_add(1, SeqCst);
        if ARMED.load(SeqCst) {
            // SAFETY: `ARMED` is true, so the forms are not being written.
            let armed = unsafe { &*FORMS.0.get() };
            if layout.size() >= armed.min_len {
                // SAFETY: the block is still allocated and `layout.size()`
                // bytes long. Bytes the program never wrote may be read here;
                // the scan only compares them.
                let block = unsafe { std::slice::from_raw_parts(pointer, layout.size()) };
                scan_block(armed, block);
            }
        }
        IN_FLIGHT.fetch_sub(1, SeqCst);
        unsafe { System.dealloc(pointer, layout) }
    }
}

/// Count one hit for each canary that has a form in `block`. Allocates nothing.
fn scan_block(armed: &ArmedFormsInner, block: &[u8]) {
    let mut found = [false; MAX_CANARIES];
    for i in 0..armed.count {
        let owner = armed.owners[i];
        if found[owner] {
            continue;
        }
        let form = armed.forms[i].as_slice();
        if form.len() <= block.len() && contains(block, form) {
            found[owner] = true;
            HITS[owner].fetch_add(1, SeqCst);
        }
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    let first = needle[0];
    haystack
        .windows(needle.len())
        .any(|window| window[0] == first && window == needle)
}

/// Holds [`SCAN_LOCK`] while a scan is armed. Dropping it disarms, even when
/// the code under test panics.
struct Armed {
    canaries: usize,
    _lock: MutexGuard<'static, ()>,
}

impl Armed {
    fn arm(canaries: &[Canary]) -> Self {
        assert!(
            !canaries.is_empty() && canaries.len() <= MAX_CANARIES,
            "a scan arms 1 to {MAX_CANARIES} canaries, got {}",
            canaries.len()
        );
        let lock = SCAN_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        debug_assert!(!ARMED.load(SeqCst));
        // SAFETY: we hold `SCAN_LOCK` and `ARMED` is false. The last disarm
        // waited for `IN_FLIGHT` to reach zero, so no free reads the forms.
        let armed = unsafe { &mut *FORMS.0.get() };
        armed.count = 0;
        armed.min_len = usize::MAX;
        for (owner, canary) in canaries.iter().enumerate() {
            for form in canary.forms() {
                armed.forms[armed.count] = *form;
                armed.owners[armed.count] = owner;
                armed.count += 1;
                armed.min_len = armed.min_len.min(form.len);
            }
        }
        for hits in &HITS {
            hits.store(0, SeqCst);
        }
        ARMED.store(true, SeqCst);
        Self {
            canaries: canaries.len(),
            _lock: lock,
        }
    }

    fn disarm() {
        ARMED.store(false, SeqCst);
        while IN_FLIGHT.load(SeqCst) != 0 {
            std::hint::spin_loop();
        }
    }

    /// Disarm and read the hit counts.
    fn finish(self) -> Vec<usize> {
        Self::disarm();
        HITS[..self.canaries]
            .iter()
            .map(|hits| hits.load(SeqCst))
            .collect()
    }
}

impl Drop for Armed {
    fn drop(&mut self) {
        Self::disarm();
    }
}

/// Run `f` with `canaries` armed. Returns its output and the hit count of each
/// canary, in argument order.
///
/// # Panics
///
/// If `canaries` is empty or holds more than [`MAX_CANARIES`].
pub fn scan<T>(canaries: &[Canary], f: impl FnOnce() -> T) -> (T, Vec<usize>) {
    let armed = Armed::arm(canaries);
    let output = f();
    (output, armed.finish())
}

/// Await `f` with `canaries` armed. Returns its output and the hit count of
/// each canary, in argument order. Frees on other tokio workers count.
///
/// # Panics
///
/// If `canaries` is empty or holds more than [`MAX_CANARIES`].
pub async fn scan_async<T>(canaries: &[Canary], f: impl Future<Output = T>) -> (T, Vec<usize>) {
    let armed = Armed::arm(canaries);
    let output = f.await;
    (output, armed.finish())
}

/// Install the heap-canary [`Scanner`] as this binary's global allocator.
///
/// Call it once, at the top level of a test binary. The library never
/// installs it.
#[macro_export]
macro_rules! install_heap_canary {
    () => {
        #[global_allocator]
        static XMTP_HEAP_CANARY: $crate::secret::heap_canary::Scanner =
            $crate::secret::heap_canary::Scanner;
    };
}
