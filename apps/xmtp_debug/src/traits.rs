pub trait IsReadOnly {
    fn is_read_only(&self) -> bool;
}

impl<T> IsReadOnly for Option<T>
where
    T: IsReadOnly,
{
    fn is_read_only(&self) -> bool {
        if let Some(ref v) = self {
            v.is_read_only()
        }
    }
}
