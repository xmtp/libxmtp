//! Message contents that tests build.

/// Wrap content in compressed replies, optionally padded with valid unknown protobuf fields.
pub fn nested_reply(
    mut content: xmtp_proto::xmtp::mls::message_contents::EncodedContent,
    levels: usize,
    pad_to: usize,
) -> xmtp_proto::xmtp::mls::message_contents::EncodedContent {
    use xmtp_content_types::{
        ContentCodec,
        compression::compress,
        reply::{Reply as EncodedReply, ReplyCodec},
    };
    use xmtp_proto::xmtp::mls::message_contents::Compression;

    for _ in 0..levels {
        let mut reply = ReplyCodec::encode(EncodedReply {
            reference: "0102".into(),
            reference_inbox_id: None,
            content,
        })
        .expect("an encoded reply");
        if pad_to > reply.content.len() + 6 {
            // An unknown protobuf field pads this reply without changing its body.
            let padding = pad_to - reply.content.len() - 6;
            reply.content.extend_from_slice(&[0xa2, 0x06]);
            let mut length = padding;
            while length >= 0x80 {
                reply.content.push((length as u8) | 0x80);
                length >>= 7;
            }
            reply.content.push(length as u8);
            reply.content.resize(reply.content.len() + padding, 0);
        }
        content = compress(reply, Compression::Deflate).expect("a compressed reply");
    }
    content
}
