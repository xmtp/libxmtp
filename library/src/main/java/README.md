# LibXMTP Kotlin

Kotlin code emitted by the `bindings_ffi` crate in [libxmtp](https://github.com/xmtp/libxmtp) including how to get jni libraries

## Process for updating from a [libxmtp](https://github.com/xmtp/libxmtp) Kotlin Binding Release (work in progress!)

1. From repo [libxmtp](https://github.com/xmtp/libxmtp) checkout the branch you would like to make a release from
2. Navigate to the `bindings_ffi` folder
3. Follow the instructions for "Rebuilding this crate in the `bindings_ffi` [README](https://github.com/xmtp/libxmtp/tree/main/bindings_ffi#rebuilding-this-crate)"
4. Copy the contents of `libxmtp/bindings_ffi/src/uniffi/xmtpv3/xmtpv3.kt` to `xmtp-android/library/src/main/java/xmtpv3.kt`
5. All instances of `value.forEach` should be changed to `value.iterator().forEach` to be compatible with API 23
6. Copy the jniLibs from `libxmtp/bindings_ffi/jniLibs` to `xmtp-android/library/src/main/jniLibs`

You should now be on the latest libxmtp. Tests will fail if the jniLibs do not match the version of xmtpv3.