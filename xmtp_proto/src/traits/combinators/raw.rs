pub struct Raw<E> {
    endpoint: E,
    _marker: PhantomData<()>,
}


#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<E, C> Query<(), C> for Raw<E>
where
    E: Query<T, C> + Send + Sync,
    C: Client + Send + Sync,
{
    async fn query(&mut self, client: &C) -> Result<(), ApiClientError<C::Error>> {
        let _ = Query::<T, C>::query(&mut self.endpoint, client).await?;
        // ignore response value
        Ok(())
    }
}


