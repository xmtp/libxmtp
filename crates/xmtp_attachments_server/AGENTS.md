# xmtp_attachments_server

This crate builds an S3-compatible storage target and signs PUT requests. Its
settings are `xmtp_backend_config::attachments`, which owns their checks.
`build_target` and `S3Target::new` apply those checks again, because they are
public and take settings that any caller can build. Run
`dev/nix-shell 'just test crate xmtp_attachments_server'` from the repository
root. Do not log credentials or presigned URLs.
