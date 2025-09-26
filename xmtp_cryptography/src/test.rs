use openmls::test_utils::frankenstein::FrankenPublicMessage;
use xmtp_common::Generate;

struct XmtpCryptography;

impl Generate<XmtpCryptography> for FrankenPublicMessage {}
