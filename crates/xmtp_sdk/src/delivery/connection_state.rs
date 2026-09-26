use xmtp_mls::subscriptions::incoming::IncomingConnection;

/// Transport state for one subscription reader.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Reconnecting,
    Failed,
    Closed,
}

impl From<IncomingConnection> for ConnectionState {
    fn from(value: IncomingConnection) -> Self {
        match value {
            IncomingConnection::Connecting => Self::Connecting,
            IncomingConnection::Connected => Self::Connected,
            IncomingConnection::Reconnecting => Self::Reconnecting,
            IncomingConnection::Failed => Self::Failed,
            IncomingConnection::Closed => Self::Closed,
        }
    }
}
