//! External-invite primitives for QR-code-based group joining via MLS external
//! commits.
//!
//! The [`payload`] module provides helpers for the
//! [`ExternalInvitePayload`] proto. The [`encrypted_group_info`] module
//! provides the encryption envelope.
//!
//! [`ExternalInvitePayload`]: xmtp_proto::xmtp::mls::message_contents::ExternalInvitePayload

pub mod encrypted_group_info;
pub mod payload;
