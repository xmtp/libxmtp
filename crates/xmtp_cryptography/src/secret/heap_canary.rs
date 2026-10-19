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
//! - A freed block can hold uninitialized bytes: fresh allocations, spare
//!   `Vec` capacity, and struct padding after a typed copy. Reading those
//!   through a Rust `&[u8]` is undefined behavior, so the scanner never forms a
//!   Rust reference to a freed block. It searches the block through
//!   `libc::memmem` on raw pointers; C reads every byte as `unsigned char`,
//!   which has no invalid values. Only the canary forms, which live in a static,
//!   are read as Rust slices. A general allocator sees only a pointer and a
//!   size, so it cannot track which bytes of a block were written; reading the
//!   whole block as `unsigned char` in C is the sound alternative.
//! - Disarming wipes the armed forms, so the raw key does not stay in the
//!   static after a scan.
//! - `realloc` keeps the default `alloc` + copy + `dealloc`, so a block that
//!   grows while it holds the secret is scanned when the old block is freed.
//!   Never forward `realloc` to `System.realloc`: the old block would be freed
//!   without a scan.
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

use super::Zeroize;

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
// block, through `libc::memmem`, before handing it back.
unsafe impl GlobalAlloc for Scanner {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        IN_FLIGHT.fetch_add(1, SeqCst);
        if ARMED.load(SeqCst) {
            // SAFETY: `ARMED` is true, so the forms are not being written.
            let armed = unsafe { &*FORMS.0.get() };
            if layout.size() >= armed.min_len {
                // SAFETY: the block is still allocated and `layout.size()`
                // bytes long; `scan_block` reads it only through `memmem`.
                unsafe { scan_block(armed, pointer, layout.size()) };
            }
        }
        IN_FLIGHT.fetch_sub(1, SeqCst);
        unsafe { System.dealloc(pointer, layout) }
    }
}

/// Count one hit for each canary that has a form in the block at `pointer`.
/// Allocates nothing.
///
/// # Safety
/// `pointer` must be valid for reads of `len` bytes. The bytes may be
/// uninitialized: they are only read by `libc::memmem`.
unsafe fn scan_block(armed: &ArmedFormsInner, pointer: *const u8, len: usize) {
    let mut found = [false; MAX_CANARIES];
    for i in 0..armed.count {
        let owner = armed.owners[i];
        if found[owner] {
            continue;
        }
        let form = armed.forms[i].as_slice();
        // SAFETY: the caller guarantees `pointer` is readable for `len` bytes.
        if form.len() <= len && unsafe { contains(pointer, len, form) } {
            found[owner] = true;
            HITS[owner].fetch_add(1, SeqCst);
        }
    }
}

/// Whether `needle` occurs in the `len` bytes at `haystack`. The haystack is
/// passed to C as a raw pointer, so no Rust reference to possibly
/// uninitialized bytes is formed.
///
/// # Safety
/// `haystack` must be valid for reads of `len` bytes.
unsafe fn contains(haystack: *const u8, len: usize, needle: &[u8]) -> bool {
    // SAFETY: `haystack` is readable for `len` bytes (caller) and `needle` is a
    // live slice; `memmem` only reads both.
    let found = unsafe {
        libc::memmem(
            haystack.cast::<libc::c_void>(),
            len,
            needle.as_ptr().cast::<libc::c_void>(),
            needle.len(),
        )
    };
    !found.is_null()
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

    /// Stop scanning, wait for in-flight frees, then wipe the armed forms so
    /// the raw key does not stay in static memory. Callers hold `SCAN_LOCK`
    /// through the `Armed` guard.
    fn disarm() {
        ARMED.store(false, SeqCst);
        while IN_FLIGHT.load(SeqCst) != 0 {
            std::hint::spin_loop();
        }
        // SAFETY: `ARMED` is false and no free is in flight, and the caller
        // holds `SCAN_LOCK`, so nothing else reads or writes the forms.
        let armed = unsafe { &mut *FORMS.0.get() };
        for form in &mut armed.forms {
            form.bytes.zeroize();
            form.len = 0;
        }
        armed.count = 0;
        armed.min_len = usize::MAX;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The armed forms hold the raw key. Disarming wipes them; without the
    /// wipe the key stays in the static until the next scan overwrites it.
    #[test]
    fn disarm_wipes_the_armed_forms() {
        let key = [0x5a; 32];
        let ((), _) = scan(&[Canary::new(&key)], || ());
        let _lock = SCAN_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        // SAFETY: no scan is armed and we hold `SCAN_LOCK`.
        let armed = unsafe { &*FORMS.0.get() };
        assert_eq!(armed.count, 0);
        assert!(
            armed
                .forms
                .iter()
                .all(|form| form.len == 0 && form.bytes.iter().all(|&b| b == 0))
        );
    }
}
