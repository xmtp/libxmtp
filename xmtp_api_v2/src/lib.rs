trait Endpoint {
    fn http_endpoint(&self) -> Cow<'static, str>;

    fn grpc_endpoint(&self) -> Cow<'static, str>;

    fn body(&self) -> Result<Vec<u8>, BodyError>;
}

trait Client {
    type Error: Error + Send + Sync + 'static;

    async fn request(
        &self,
        request: RequestBuilder,
        body: Vec<u8>,
    ) -> Result<Response<Bytes>, ApiError<Self::Error>>;

    async fn stream(
        &self,
        request: RequestBuilder,
        body: Vec<u8>,
    ) -> Result<Response<Bytes>, ApiError<Self::Error>>;
}

// query can return a Wrapper XmtpResponse<T> that implements both Future and Stream. If stream is used on singular response, just a stream of one item. This lets us re-use query for everything.
trait Query<T, C> {
    async fn query(&self, client: &C) -> Result<T, ApiError<C::Error>>;
}

/*
// blanket Query implementation for a bare Endpoint
impl<E, T, C> Query<T, C> for E
where
    E: Endpoint,
    T: TryInto,
    C: Client,
{
    /* ... */
}
*/
