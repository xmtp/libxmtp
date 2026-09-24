#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub mod native;

use diesel::SqliteConnection;
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub use native::*;

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub mod wasm;
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub use wasm::*;

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub use wasm_exports::*;

#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub use native_exports::*;

use super::ConnectionExt;

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub mod wasm_exports {
    pub type RawDbConnection = diesel::prelude::SqliteConnection;
    pub type DefaultDatabase = super::wasm::WasmDb;
}

#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub mod native_exports {
    pub type DefaultDatabase = super::native::NativeDb;
    pub use super::native::EncryptedConnection;
}

mod instrumentation;

#[derive(Debug)]
pub enum PersistentOrMem<P, S, M> {
    Persistent(P),
    Single(S),
    Mem(M),
}

// P, S and M must share connection & error types
impl<P, S, M> ConnectionExt for PersistentOrMem<P, S, M>
where
    P: ConnectionExt,
    S: ConnectionExt,
    M: ConnectionExt,
{
    fn raw_query<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        match self {
            Self::Persistent(p) => p.raw_query(fun),
            Self::Single(s) => s.raw_query(fun),
            Self::Mem(m) => m.raw_query(fun),
        }
    }

    fn disconnect(&self) -> Result<(), crate::ConnectionError> {
        match self {
            Self::Persistent(p) => p.disconnect(),
            Self::Single(s) => s.disconnect(),
            Self::Mem(m) => m.disconnect(),
        }
    }

    fn reconnect(&self) -> Result<(), crate::ConnectionError> {
        match self {
            Self::Persistent(p) => p.reconnect(),
            Self::Single(s) => s.reconnect(),
            Self::Mem(m) => m.reconnect(),
        }
    }
}

/// `std::convert::Infallible` is used as the `Single` type parameter on targets
/// (wasm) that have no single-connection mode. It is uninhabited, so the
/// `Single` arm is statically impossible to construct.
impl ConnectionExt for std::convert::Infallible {
    fn raw_query<T, F>(&self, _fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        match *self {}
    }

    fn disconnect(&self) -> Result<(), crate::ConnectionError> {
        match *self {}
    }

    fn reconnect(&self) -> Result<(), crate::ConnectionError> {
        match *self {}
    }
}

#[cfg(test)]
mod persistent_or_mem_tests {
    use super::*;

    // A trivial in-memory ConnectionExt used to exercise enum dispatch without a real DB.
    struct CountingConn;
    impl ConnectionExt for CountingConn {
        fn raw_query<T, F>(&self, _fun: F) -> Result<T, crate::ConnectionError>
        where
            F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
            Self: Sized,
        {
            // Not exercised in this test; we only verify disconnect/reconnect dispatch.
            unreachable!("raw_query not used in this test")
        }
        fn disconnect(&self) -> Result<(), crate::ConnectionError> {
            Ok(())
        }
        fn reconnect(&self) -> Result<(), crate::ConnectionError> {
            Ok(())
        }
    }

    #[test]
    fn single_arm_dispatches() {
        // Single arm with a real (CountingConn) type dispatches correctly.
        let c: PersistentOrMem<CountingConn, CountingConn, CountingConn> =
            PersistentOrMem::Single(CountingConn);
        assert!(c.disconnect().is_ok());
        assert!(c.reconnect().is_ok());
    }

    #[test]
    fn infallible_single_arm_compiles() {
        // The wasm-shaped type: Single = Infallible. Must construct a non-Single arm.
        let c: PersistentOrMem<CountingConn, std::convert::Infallible, CountingConn> =
            PersistentOrMem::Mem(CountingConn);
        assert!(c.disconnect().is_ok());
    }
}
