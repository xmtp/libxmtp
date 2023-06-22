pub type Address = String;

pub mod networking {
    use async_trait::async_trait;
    use std::{error::Error as StdError, fmt};

    pub use xmtp_proto::xmtp::message_api::v1::{
        Envelope, PagingInfo, PublishRequest, PublishResponse, QueryRequest, QueryResponse,
        SubscribeRequest,
    };

    #[derive(Debug)]
    pub enum ErrorKind {
        SetupError,
        PublishError,
        QueryError,
        SubscribeError,
    }

    type ErrorSource = Box<dyn StdError + Send + Sync + 'static>;

    pub struct Error {
        kind: ErrorKind,
        source: Option<ErrorSource>,
    }

    impl Error {
        pub fn new(kind: ErrorKind) -> Self {
            Self { kind, source: None }
        }

        pub fn with(mut self, source: impl Into<ErrorSource>) -> Self {
            self.source = Some(source.into());
            self
        }
    }

    impl fmt::Debug for Error {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let mut f = f.debug_tuple("xmtp::error::Error");

            f.field(&self.kind);

            if let Some(source) = &self.source {
                f.field(source);
            }

            f.finish()
        }
    }

    impl fmt::Display for Error {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(match &self.kind {
                ErrorKind::SetupError => "setup error",
                ErrorKind::PublishError => "publish error",
                ErrorKind::QueryError => "query error",
                ErrorKind::SubscribeError => "subscribe error",
            })
        }
    }

    impl StdError for Error {
        fn source(&self) -> Option<&(dyn StdError + 'static)> {
            self.source
                .as_ref()
                .map(|source| &**source as &(dyn StdError + 'static))
        }
    }

    pub trait XmtpApiSubscription {
        fn is_closed(&self) -> bool;
        fn get_messages(&self) -> Vec<Envelope>;
        fn close_stream(&mut self);
    }

    #[async_trait]
    pub trait XmtpApiClient {
        type Subscription: XmtpApiSubscription;

        async fn publish(
            &self,
            token: String,
            request: PublishRequest,
        ) -> Result<PublishResponse, Error>;

        async fn query(&self, request: QueryRequest) -> Result<QueryResponse, Error>;

        async fn subscribe(&self, request: SubscribeRequest) -> Result<Self::Subscription, Error>;
    }
}
