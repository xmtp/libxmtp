#!/bin/bash

CMDS="-b dev"
XDBG_PRE="nix run github:xmtp/libxmtp/a0f198b#xdbg -- $CMDS"

#XDBG_LATEST="nix run github:xmtp/libxmtp/cc0ddb9358288710efc73bbe44e1cc8e2ec9c238#xdbg -- $CMDS"
XDBG_LATEST="nix run github:xmtp/libxmtp/3e15f8f3ef068053f49f9571f8d0de81bb9302ca#xdbg -- $CMDS"

$XDBG_PRE --version
$XDBG_LATEST --version

$XDBG_PRE --clear
$XDBG_LATEST --clear
rm *.json

$XDBG_PRE generate --entity identity -a 2
$XDBG_PRE generate --entity group -a 1 --invite 2
$XDBG_PRE -vvvv --json generate --entity message -a 1

mv *.json pre.json
# upgrade to latest
$XDBG_LATEST -vvvv --json generate --entity message -a 5

mv 2026-*.json post.json
