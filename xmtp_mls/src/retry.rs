use smart_default::SmartDefault;

pub trait RetryableError: std::error::Error {
    fn is_retryable(&self) -> bool {
        false
    }
}

// we use &T as a workaround for specialization here
impl<T> RetryableError for &T where T: std::error::Error {}

#[derive(SmartDefault, PartialEq, Eq, Copy, Clone)]
pub struct Retry {
    #[default = 3]
    retries: usize,
    #[default(_code = "std::time::Duration::from_millis(100)")]
    duration: std::time::Duration,
}

#[derive(Default, PartialEq, Eq, Copy, Clone)]
pub struct RetryBuilder {
    retries: Option<usize>,
    duration: Option<std::time::Duration>,
}

impl RetryBuilder {
    pub fn retries(mut self, retries: usize) -> Self {
        self.retries = Some(retries);
        self
    }

    pub fn duration(mut self, duration: std::time::Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    pub fn build(self) -> Retry {
        let mut retry: Retry = Default::default();

        if let Some(retries) = self.retries {
            retry.retries = retries;
        }

        if let Some(duration) = self.duration {
            retry.duration = duration;
        }

        retry
    }
}

impl Retry {
    pub fn builder() -> RetryBuilder {
        RetryBuilder::default()
    }

    pub fn retry<F, T, E>(&self, fun: F) -> Result<T, E>
    where
        E: RetryableError,
        F: FnOnce(Retry),
    {
        todo!()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum RetryableResult<T, E> {
    Ok(T),
    Retry(E),
    Err(E),
}

impl<T, E> From<Result<T, E>> for RetryableResult<T, E>
where
    E: RetryableError,
{
    fn from(res: Result<T, E>) -> RetryableResult<T, E> {
        match res {
            Result::Ok(value) => RetryableResult::Ok(value),
            Result::Err(e) => {
                if e.is_retryable() {
                    RetryableResult::Retry(e)
                } else {
                    RetryableResult::Err(e)
                }
            }
        }
    }
}
