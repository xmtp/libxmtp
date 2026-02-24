#!/bin/bash

RED='\033[0;31m'
NC='\033[0m' # No Color
XMTP_SWIFT="${1:-$(realpath ../../libxmtp-swift)}"

if [ ! -d $XMTP_SWIFT ]; then
  echo "${RED}libxmtp-swift directory not detected${NC}"
  echo "${RED}Ensure \`github.com/xmtp/libxmtp-swift\` is cloned as a sibling directory or passed as the first argument to this script.${NC}"
  exit
fi
echo "Swift Directory: $XMTP_SWIFT"

# Assumes libxmtp is in a peer directory of libxmtp-swift
make swift
cp build/swift/xmtpv3.swift $XMTP_SWIFT/Sources/LibXMTP/xmtpv3.swift
cp build/swift/libxmtp-version.txt $XMTP_SWIFT/Sources/LibXMTP/libxmtp-version.txt
