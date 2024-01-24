# Assumes libxmtp is in a peer directory of libxmtp-swift
make swiftlocal ; rm -rf ../../libxmtp-swift/LibXMTPRust.xcframework
cp -r ./LibXMTPRust.xcframework ../../libxmtp-swift/LibXMTPRust.xcframework
cp build/swift/xmtpv3.swift ../../libxmtp-swift/Sources/LibXMTP/xmtpv3.swift
