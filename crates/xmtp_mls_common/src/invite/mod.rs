//! External-invite primitives for QR-code-based group joining via MLS external
//! commits.
//!
//! The [`payload`] module provides helpers for the
//! [`ExternalInvitePayload`] proto. The encryption envelope lives in the
//! sibling `encrypted_group_info` module (added by a separate PR).
//!
//! [`ExternalInvitePayload`]: xmtp_proto::xmtp::mls::message_contents::ExternalInvitePayload

pub mod payload;
