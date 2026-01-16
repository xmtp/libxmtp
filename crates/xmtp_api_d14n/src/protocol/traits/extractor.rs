/// An Extractor indicates a specific direction to
/// process an [`ProtocolEnvelope`].
/// Extractor implementations are available in [`crate::protocol::extractors`]
pub trait Extractor {
    /// The output this extractor will produce
    type Output;
    /// Get the output of the extraction
    fn get(self) -> Self::Output;
}

/// Represents an [`Extractor`] whose output is a [`Result`]
/// Useful for deriving traits that should be aware of Result Ok and Error
/// values.
pub trait TryExtractor: Extractor<Output = Result<Self::Ok, Self::Error>> {
    /// The [`Result::Ok`] value of an [`Extractor`]
    type Ok;
    /// The [`Result::Err`] value of an [`Extractor`]
    type Error;
    /// Try to get the extraction result
    fn try_get(self) -> Result<Self::Ok, Self::Error>;
}

impl<T, O, Err> TryExtractor for T
where
    T: Extractor<Output = Result<O, Err>>,
{
    type Ok = O;

    type Error = Err;

    fn try_get(self) -> Result<Self::Ok, Self::Error> {
        self.get()
    }
}

pub trait MaybeExtractor: Extractor<Output = Option<Self::Value>> {
    type Value;
    fn maybe_get(self) -> Option<Self::Value>;
}

impl<T, V> MaybeExtractor for T
where
    T: Extractor<Output = Option<V>>,
{
    type Value = V;

    fn maybe_get(self) -> Option<Self::Value> {
        self.get()
    }
}
