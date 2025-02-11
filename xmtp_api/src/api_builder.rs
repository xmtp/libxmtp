pub trait ApiBuilder {
    type Output;
    type Error;

    fn set_libxmtp_version(&mut self, version: String) -> Result<(), Self::Error>;

    fn set_app_version(&mut self, version: String) -> Result<(), Self::Error>;

    fn set_address(&mut self, address: String) -> Result<(), Self::Error>;
}
