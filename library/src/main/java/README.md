# LibXMTP Kotlin

Kotlin code emitted by the `bindings_ffi` crate in [libxmtp](https://github.com/xmtp/libxmtp) including how to get jni libraries

## Process for updating from a [libxmtp](https://github.com/xmtp/libxmtp) Kotlin Binding Release

1. From repo [libxmtp](https://github.com/xmtp/libxmtp) run the [kotlin release action](https://github.com/xmtp/libxmtp/actions/workflows/release-kotlin-bindings.yml) for the branch you desire 
2. Create a new branch in the `xmtp-android` repo
   With `libxmtp` repo and `xmtp-android` (this repo) cloned locally in sibling directories, and `libxmtp` checked out to the correct release commit, run the script:
   `./bindings_ffi/gen_kotlin.sh`
3. Run format (cmd + opt + l) function to keep the code format consistent and diff small for `xmtp-android/library/src/main/java/xmtpv3.kt`
4. Navigate to the [latest release](https://github.com/xmtp/libxmtp/releases) once the action completes
5. Download the `LibXMTPKotlinFFI.zip` assets
6. Unzip and then copy the jniLibs to `xmtp-android/library/src/main/jniLibs`

You should now be on the latest libxmtp. Tests will fail if the jniLibs do not match the version of xmtpv3.