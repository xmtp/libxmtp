pub struct ClientBundle<V3, Xmtpd, Gateway> {
    v3: V3,
    xmtpd: Xmtpd,
    gateway: Gateway,
}

pub trait ClientBundleProvider {}
