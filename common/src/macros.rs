/// Turn the `Result<T, E>` into an `Option<T>`, logging the error with `tracing::error` and
/// returning `None` if the value matches on Result::Err().
/// Optionally pass a message as the second argument.
#[macro_export]
macro_rules! optify {
    ( $e: expr ) => {
        match $e {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::error!("{}", e);
                None
            }
        }
    };
    ( $e: expr, $msg: tt ) => {
        match $e {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::error!("{}: {:?}", $msg, e);
                None
            }
        }
    };
}

/// Convenience macro to easily export items for wasm
#[macro_export]
macro_rules! if_wasm {
    ($($item:item)*) => {$(
        #[cfg(all(target_family = "wasm", target_os = "unknown"))]
        $item
    )*}
}

/// Convenience macro to easily export items for native
#[macro_export]
macro_rules! if_native {
    ($($item:item)*) => {$(
        #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
        $item
    )*}
}

/// Convenience macro to easily export items for d14n
#[macro_export]
macro_rules! if_d14n {
    ($($item:item)*) => {$(
        #[cfg(feature = "d14n")]
        $item
    )*}
}

/// Convenience macro to easily export items for d14n
#[macro_export]
macro_rules! if_v3 {
    ($($item:item)*) => {$(
        #[cfg(not(feature = "d14n"))]
        $item
    )*}
}

/// Feature flag for dev network
#[macro_export]
macro_rules! if_dev {
    ($($item:item)*) => {$(
        #[cfg(feature = "dev")]
        $item
    )*}
}

#[macro_export]
macro_rules! if_local {
    ($($item:item)*) => {$(
        #[cfg(not(feature = "dev"))]
            $item
    )*}
}

#[macro_export]
macro_rules! if_test {
    ($($item:item)*) => {$(
        #[cfg(any(test, feature = "test-utils"))]
        $item
    )*}
}

// cfg only test but not any extra test-utils features
#[macro_export]
macro_rules! if_only_test {
    ($($item:item)*) => {$(
        #[cfg(test)]
        $item
    )*}
}
