/// Turn the `Result<T, E>` into an `Option<T>`, logging the error with `tracing::error` and
/// returning `None` if the value matches on Result::Err().
/// Optionally pass a message as the second argument.
#[macro_export]
macro_rules! optify {
    ( $e: expr_2021 ) => {
        match $e {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::error!("{}", e);
                None
            }
        }
    };
    ( $e: expr_2021, $msg: tt ) => {
        match $e {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::error!("{}: {:?}", $msg, e);
                None
            }
        }
    };
}
