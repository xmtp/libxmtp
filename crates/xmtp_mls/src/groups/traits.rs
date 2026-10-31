// A minimal, ergonomic, and generic pattern for "conversion with extra parameters" in Rust.
// - `FromWith<T>` mirrors `From<T>` but lets each TARGET type decide what extra params it needs
//   via an associated type (`Params`). Call sites don’t have to specify the params type.
// - `IntoWith<Target>` mirrors `Into<Target>` for nice `.into_with(&params)` syntax.
// - `?Sized` on `Params` allows using trait objects or slices as parameters (e.g., `&dyn Cfg`).
pub trait FromWith<T>: Sized {
    /// Each target picks its own parameter type.
    /// `?Sized` lets you use `&dyn Trait` or `&[T]` instead of concrete types.
    type Params: ?Sized;

    /// Build `Self` from `value` and additional `params`.
    fn from_with(value: T, params: &Self::Params) -> Self;
}

pub trait IntoWith<Target>: Sized {
    /// Uses the same parameter type as `Target::Params`.
    type Params: ?Sized;

    /// Convert `self` into `Target` using extra `params`, in `.into_with(...)` style.
    fn into_with(self, params: &Self::Params) -> Target;
}

// Blanket impl so every `T` can call `.into_with::<Target>(&params)`
// whenever `Target: FromWith<T>`.
impl<T, Target> IntoWith<Target> for T
where
    Target: FromWith<T>,
{
    type Params = <Target as FromWith<T>>::Params;

    #[inline]
    fn into_with(self, params: &Self::Params) -> Target {
        Target::from_with(self, params)
    }
}
