pub trait IntoCached {
    type Output;
    fn into_cached(&self) -> Self::Output;
}
