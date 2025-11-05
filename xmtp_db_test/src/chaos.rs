use derive_builder::Builder;
use diesel::SqliteConnection;
use parking_lot::Mutex;
use rand::{Rng, distributions::Standard, prelude::Distribution};
use std::{collections::HashMap, sync::Arc};
use xmtp_common::{MaybeSend, MaybeSync, if_native, if_wasm};
use xmtp_db::ConnectionExt;

const TRANSACTION_START_HOOK: &str = "TRANSACTION_START_HOOK";
const PRE_READ_HOOK: &str = "PRE_READ_HOOK";
const PRE_WRITE_HOOK: &str = "PREWRITE_HOOK";

const POST_READ_HOOK: &str = "POST_READ_HOOK";
const POST_WRITE_HOOK: &str = "POST_WRITE_HOOK";

// --------------------------------------------------------
// /\/\/\/\/\/\/\/\/\/\||Static Hooks||\/\/\/\/\/\/\/\/\/\/\
// --------------------------------------------------------

const STATIC_TRANSACTION_START_HOOK: &str = "STATIC_TRANSACTION_START_HOOK";

const STATIC_PRE_READ_HOOK: &str = "STATIC_PRE_READ_HOOK";
const STATIC_PRE_WRITE_HOOK: &str = "STATIC_PRE_WRITE_HOOK";

const STATIC_POST_READ_HOOK: &str = "STATIC_POST_READ_HOOK";
const STATIC_POST_WRITE_HOOK: &str = "STATIC_POST_WRITE_HOOK";

if_native! {
    type HookFn<C> = Box<dyn Fn(&C) -> Result<(), xmtp_db::ConnectionError> + Send + Sync>;
}
if_wasm! {
    type HookFn<C> = Box<dyn Fn(&C) -> Result<(), xmtp_db::ConnectionError>>;
}

#[derive(Builder)]
#[builder(setter(into), build_fn(validate = "Self::validate"))]
#[allow(clippy::type_complexity)]
pub struct ChaosConnection<C> {
    db: C,
    #[builder(setter(skip), default)]
    hooks: Arc<Mutex<HashMap<&'static str, Vec<HookFn<C>>>>>,
    #[builder(setter(skip), default)]
    static_hooks: Arc<Mutex<HashMap<&'static str, Vec<HookFn<C>>>>>,
    /// Set a probability for errors to occur when running transactions
    #[builder(default = "0.0")]
    error_frequency: f64,
}

impl<C> Clone for ChaosConnection<C>
where
    C: Clone,
{
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            hooks: self.hooks.clone(),
            static_hooks: self.static_hooks.clone(),
            error_frequency: self.error_frequency,
        }
    }
}

impl<C> ChaosConnection<C>
where
    C: Clone,
{
    pub fn builder() -> ChaosConnectionBuilder<C> {
        Default::default()
    }

    pub fn set_error_frequency(&mut self, frequency: f64) {
        if !(0.0..1.0).contains(&frequency) {
            panic!("error_frequency must be a value between 0.0 and 1.0 (EX: 0.40)");
        }
        self.error_frequency = frequency;
    }
}

impl<C> ChaosConnectionBuilder<C> {
    // validate that the frequency is between the correct values
    fn validate(&self) -> Result<(), String> {
        // ensure error frequency is a percentage
        if let Some(frequency) = self.error_frequency
            && !(0.0..1.0).contains(&frequency)
        {
            return Err(
                "error_frequency must be a value between 0.0 and 1.0 (EX: 0.40)".to_string(),
            );
        }
        Ok(())
    }
}

impl<C> ChaosConnection<C> {
    pub fn get_mod(&self, hook: &'static str) -> Option<HookFn<C>> {
        let mut m = self.hooks.lock();
        m.get_mut(hook).map(|h| h.pop())?
    }

    pub fn run_hook(&self, hook: &'static str) -> Result<(), xmtp_db::ConnectionError> {
        if let Some(f) = self.get_mod(hook) {
            f(&self.db)?;
        }
        Ok(())
    }

    pub fn run_static_hooks(&self, hook: &'static str) -> Result<(), xmtp_db::ConnectionError> {
        let h = self.static_hooks.lock();
        if let Some(f) = h.get(hook) {
            f.iter().try_for_each(|h| h(&self.db))?;
        }
        Ok(())
    }

    /// Add a hook to run after the next transaction is started
    pub fn start_transaction_hook<F>(&self, f: F)
    where
        F: Fn(&C) -> Result<(), xmtp_db::ConnectionError> + MaybeSend + MaybeSync + 'static,
    {
        let mut m = self.hooks.lock();
        m.entry(TRANSACTION_START_HOOK)
            .or_default()
            .push(Box::new(f));
    }

    /// Add a hook to run before the next read
    pub fn pre_read_hook<F>(&self, f: F)
    where
        F: Fn(&C) -> Result<(), xmtp_db::ConnectionError> + MaybeSend + MaybeSync + 'static,
    {
        let mut m = self.hooks.lock();
        m.entry(PRE_READ_HOOK).or_default().push(Box::new(f))
    }

    /// Add a hook to run after the next read
    pub fn post_read_hook<F>(&self, f: F)
    where
        F: Fn(&C) -> Result<(), xmtp_db::ConnectionError> + MaybeSend + MaybeSync + 'static,
    {
        let mut m = self.hooks.lock();
        m.entry(POST_READ_HOOK).or_default().push(Box::new(f))
    }

    /// Add a hook to run before the next write
    pub fn pre_write_hook<F>(&self, f: F)
    where
        F: Fn(&C) -> Result<(), xmtp_db::ConnectionError> + MaybeSend + MaybeSync + 'static,
    {
        let mut m = self.hooks.lock();
        m.entry(PRE_WRITE_HOOK).or_default().push(Box::new(f))
    }

    /// Add a hook to run after the next write
    pub fn post_write_hook<F>(&self, f: F)
    where
        F: Fn(&C) -> Result<(), xmtp_db::ConnectionError> + MaybeSend + MaybeSync + 'static,
    {
        let mut m = self.hooks.lock();
        m.entry(POST_WRITE_HOOK).or_default().push(Box::new(f))
    }

    /// Add a static hook to run on transaction start.
    /// Static hooks run on every invocation of a transaction.
    /// Static transaction hook is run before the dynamic
    /// transaction start hook.
    pub fn static_start_transaction_hook<F>(&self, f: F)
    where
        F: Fn(&C) -> Result<(), xmtp_db::ConnectionError> + MaybeSend + MaybeSync + 'static,
    {
        let mut m = self.static_hooks.lock();
        m.entry(STATIC_TRANSACTION_START_HOOK)
            .or_default()
            .push(Box::new(f));
    }

    /// Add a hook to run before every read
    /// Static hooks run on every invocation of a rea.
    /// Static hooks are run before dynamic hooks in the 'PRE' stage,
    /// but after dynamic hooks in the 'POST' stage.
    pub fn static_pre_read_hook<F>(&self, f: F)
    where
        F: Fn(&C) -> Result<(), xmtp_db::ConnectionError> + MaybeSend + MaybeSync + 'static,
    {
        let mut m = self.static_hooks.lock();
        m.entry(STATIC_PRE_READ_HOOK).or_default().push(Box::new(f))
    }

    /// Add a hook to run after every read
    /// Static hooks run on every invocation of a read.
    /// Static hooks are run before dynamic hooks in the 'PRE' stage,
    /// but after dynamic hooks in the 'POST' stage.
    pub fn static_post_read_hook<F>(&self, f: F)
    where
        F: Fn(&C) -> Result<(), xmtp_db::ConnectionError> + MaybeSend + MaybeSync + 'static,
    {
        let mut m = self.static_hooks.lock();
        m.entry(STATIC_POST_READ_HOOK)
            .or_default()
            .push(Box::new(f))
    }

    /// Add a hook to run before every write
    /// Static hooks run on every invocation of a write,
    /// Static hooks are run before dynamic hooks in the 'PRE' stage,
    /// but after dynamic hooks in the 'POST' stage.
    pub fn static_pre_write_hook<F>(&self, f: F)
    where
        F: Fn(&C) -> Result<(), xmtp_db::ConnectionError> + MaybeSend + MaybeSync + 'static,
    {
        let mut m = self.static_hooks.lock();
        m.entry(STATIC_PRE_WRITE_HOOK)
            .or_default()
            .push(Box::new(f))
    }

    /// Add a hook to run after every write
    /// Static hooks run on every invocation of a write,
    /// Static hooks are run before dynamic hooks in the 'PRE' stage,
    /// but after dynamic hooks in the 'POST' stage.
    pub fn static_post_write_hook<F>(&self, f: F)
    where
        F: Fn(&C) -> Result<(), xmtp_db::ConnectionError> + MaybeSend + MaybeSync + 'static,
    {
        let mut m = self.static_hooks.lock();
        m.entry(STATIC_POST_WRITE_HOOK)
            .or_default()
            .push(Box::new(f))
    }

    /// Possible return a random error
    /// Error return chace is decided by `error_frequency`.
    pub fn maybe_random_error<T>(&self) -> Result<(), T>
    where
        Standard: Distribution<T>,
        T: std::error::Error + xmtp_common::RetryableError,
    {
        let mut rng = rand::thread_rng();

        // Generate a random float between 0 and 1
        if rng.gen_range::<f64, _>(0.0..1.0) < self.error_frequency {
            Err(rand::random())
        } else {
            Ok(())
        }
    }
}

impl<C> ConnectionExt for ChaosConnection<C>
where
    C: ConnectionExt,
{
    fn raw_query_read<T, F>(&self, fun: F) -> Result<T, xmtp_db::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        self.run_static_hooks(STATIC_PRE_READ_HOOK)?;
        self.run_hook(PRE_READ_HOOK)?;
        self.maybe_random_error::<xmtp_db::ConnectionError>()?;
        let result = self.db.raw_query_read(fun)?;
        // TODO: we could potentially pass T into the POST hook,
        // and then the test could do some (probably unsafe) casting to
        // get a specific type out. Unsure if useful?
        self.run_hook(POST_READ_HOOK)?;
        self.run_static_hooks(STATIC_POST_READ_HOOK)?;
        Ok(result)
    }

    fn raw_query_write<T, F>(&self, fun: F) -> Result<T, xmtp_db::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        self.run_static_hooks(STATIC_PRE_WRITE_HOOK)?;
        self.run_hook(PRE_WRITE_HOOK)?;
        self.maybe_random_error::<xmtp_db::ConnectionError>()?;
        let result = self.db.raw_query_write(fun)?;
        self.run_hook(POST_WRITE_HOOK)?;
        self.run_static_hooks(STATIC_POST_WRITE_HOOK)?;
        Ok(result)
    }

    fn disconnect(&self) -> Result<(), xmtp_db::ConnectionError> {
        self.db.disconnect()
    }

    fn reconnect(&self) -> Result<(), xmtp_db::ConnectionError> {
        self.db.reconnect()
    }
}
