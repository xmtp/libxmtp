pushd xmtp_proto > /dev/null
if ! cargo install --list | grep "protoc-gen-prost-crate" > /dev/null; then
    if ! cargo install protoc-gen-prost-crate; then
        echo "Failed to install protoc-gen-prost-crate"
        exit 1
    fi
fi

# Please always specify the exact commit to use for generation!
if ! buf generate https://github.com/xmtp/proto.git#branch=main,ref=7be5c72,subdir=proto; then
    echo "Failed to generate protobuf definitions"
    exit 1
fi
popd > /dev/null
