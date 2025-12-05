/// Macro to delegate `EnvelopeVisitor` implementation to an `inner` field.
///
/// This macro generates a complete `EnvelopeVisitor` implementation for a struct
/// that contains an `inner` field implementing `EnvelopeVisitor` (typically a tuple
/// of extractors).
///
/// # Example
///
/// ```rust
/// use xmtp_api_d14n::protocol::extractors::{DependsOnExtractor, BytesExtractor};
/// use xmtp_api_d14n::delegate_envelope_visitor;
///
/// struct MyExtractor {
///     inner: (DependsOnExtractor, BytesExtractor)
/// }
///
/// delegate_envelope_visitor!(MyExtractor);
/// ```
///
/// The generated implementation delegates all visitor methods to the `inner` field,
/// allowing you to compose multiple extractors without manually implementing each
/// method. This is particularly useful when you want to wrap a tuple of extractors
/// in a named struct for better type safety and API clarity.
#[macro_export]
macro_rules! delegate_envelope_visitor {
    ($struct_name:ident) => {
        impl<'env> $crate::protocol::EnvelopeVisitor<'env> for $struct_name {
            type Error = $crate::protocol::EnvelopeError;

            fn visit_originator(
                &mut self,
                e: &xmtp_proto::xmtp::xmtpv4::envelopes::OriginatorEnvelope,
            ) -> Result<(), Self::Error> {
                self.inner.visit_originator(e)
            }

            fn visit_unsigned_originator(
                &mut self,
                e: &xmtp_proto::xmtp::xmtpv4::envelopes::UnsignedOriginatorEnvelope,
            ) -> Result<(), Self::Error> {
                self.inner.visit_unsigned_originator(e)
            }

            fn visit_payer(
                &mut self,
                e: &xmtp_proto::xmtp::xmtpv4::envelopes::PayerEnvelope,
            ) -> Result<(), Self::Error> {
                self.inner.visit_payer(e)
            }

            fn visit_client(
                &mut self,
                e: &xmtp_proto::xmtp::xmtpv4::envelopes::ClientEnvelope,
            ) -> Result<(), Self::Error> {
                self.inner.visit_client(e)
            }

            fn visit_group_message_version(
                &mut self,
                m: &xmtp_proto::xmtp::mls::api::v1::group_message_input::Version,
            ) -> Result<(), Self::Error> {
                self.inner.visit_group_message_version(m)
            }

            fn visit_group_message_input(
                &mut self,
                m: &xmtp_proto::xmtp::mls::api::v1::GroupMessageInput,
            ) -> Result<(), Self::Error> {
                self.inner.visit_group_message_input(m)
            }

            fn visit_group_message_v1(
                &mut self,
                m: &xmtp_proto::xmtp::mls::api::v1::group_message_input::V1,
            ) -> Result<(), Self::Error> {
                self.inner.visit_group_message_v1(m)
            }

            fn visit_welcome_message_version(
                &mut self,
                m: &xmtp_proto::xmtp::mls::api::v1::welcome_message_input::Version,
            ) -> Result<(), Self::Error> {
                self.inner.visit_welcome_message_version(m)
            }

            fn visit_welcome_message_input(
                &mut self,
                m: &xmtp_proto::xmtp::mls::api::v1::WelcomeMessageInput,
            ) -> Result<(), Self::Error> {
                self.inner.visit_welcome_message_input(m)
            }

            fn visit_welcome_message_v1(
                &mut self,
                m: &xmtp_proto::xmtp::mls::api::v1::welcome_message_input::V1,
            ) -> Result<(), Self::Error> {
                self.inner.visit_welcome_message_v1(m)
            }

            fn visit_welcome_pointer(
                &mut self,
                m: &xmtp_proto::xmtp::mls::api::v1::welcome_message_input::WelcomePointer,
            ) -> Result<(), Self::Error> {
                self.inner.visit_welcome_pointer(m)
            }

            fn visit_v3_group_message(
                &mut self,
                m: &xmtp_proto::xmtp::mls::api::v1::group_message::V1,
            ) -> Result<(), Self::Error> {
                self.inner.visit_v3_group_message(m)
            }

            fn visit_v3_welcome_message(
                &mut self,
                m: &xmtp_proto::xmtp::mls::api::v1::welcome_message::V1,
            ) -> Result<(), Self::Error> {
                self.inner.visit_v3_welcome_message(m)
            }

            fn visit_v3_welcome_pointer(
                &mut self,
                m: &xmtp_proto::xmtp::mls::api::v1::welcome_message::WelcomePointer,
            ) -> Result<(), Self::Error> {
                self.inner.visit_v3_welcome_pointer(m)
            }

            fn visit_upload_key_package(
                &mut self,
                p: &xmtp_proto::xmtp::mls::api::v1::UploadKeyPackageRequest,
            ) -> Result<(), Self::Error> {
                self.inner.visit_upload_key_package(p)
            }

            fn visit_identity_update(
                &mut self,
                u: &xmtp_proto::xmtp::identity::associations::IdentityUpdate,
            ) -> Result<(), Self::Error> {
                self.inner.visit_identity_update(u)
            }

            fn visit_identity_update_log(
                &mut self,
                u: &xmtp_proto::identity_v1::get_identity_updates_response::IdentityUpdateLog,
            ) -> Result<(), Self::Error> {
                self.inner.visit_identity_update_log(u)
            }

            fn visit_identity_updates_request(
                &mut self,
                u: &xmtp_proto::identity_v1::get_identity_updates_request::Request,
            ) -> Result<(), Self::Error> {
                self.inner.visit_identity_updates_request(u)
            }

            fn visit_key_package(
                &mut self,
                k: &xmtp_proto::mls_v1::fetch_key_packages_response::KeyPackage,
            ) -> Result<(), Self::Error> {
                self.inner.visit_key_package(k)
            }

            fn visit_none(&mut self) -> Result<(), Self::Error> {
                self.inner.visit_none()
            }

            fn visit_newest_envelope_response(
                &mut self,
                u: &xmtp_proto::xmtp::xmtpv4::message_api::get_newest_envelope_response::Response,
            ) -> Result<(), Self::Error> {
                self.inner.visit_newest_envelope_response(u)
            }

            fn visit_subscribe_group_messages_request(
                &mut self,
                r: &xmtp_proto::mls_v1::subscribe_group_messages_request::Filter,
            ) -> Result<(), Self::Error> {
                self.inner.visit_subscribe_group_messages_request(r)
            }

            fn visit_subscribe_welcome_messages_request(
                &mut self,
                r: &xmtp_proto::mls_v1::subscribe_welcome_messages_request::Filter,
            ) -> Result<(), Self::Error> {
                self.inner.visit_subscribe_welcome_messages_request(r)
            }

            fn visit_newest_group_message_response(
                &mut self,
                u: &xmtp_proto::xmtp::mls::api::v1::get_newest_group_message_response::Response,
            ) -> Result<(), Self::Error> {
                self.inner.visit_newest_group_message_response(u)
            }

            #[cfg(any(test, feature = "test-utils"))]
            fn test_visit_u32(&mut self, n: &u32) -> Result<(), Self::Error> {
                self.inner.test_visit_u32(n)
            }
        }
    };
}
