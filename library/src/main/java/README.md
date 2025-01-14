# LibXMTP Kotlin

Kotlin code emitted by the `bindings_ffi` crate in [libxmtp](https://github.com/xmtp/libxmtp) including how to get jni libraries

## Process for updating from a [libxmtp](https://github.com/xmtp/libxmtp) Kotlin Binding Release

1. From repo [libxmtp](https://github.com/xmtp/libxmtp) run the [kotlin release action](https://github.com/xmtp/libxmtp/actions/workflows/release-kotlin-bindings.yml) for the branch you desire (this should take about 4 minutes)
2. Once you see the [kotlin bindings GitHub action](https://github.com/xmtp/libxmtp/actions/workflows/release-kotlin-bindings.yml) is finished, with `libxmtp` repo and `xmtp-android` (this repo) cloned locally in sibling directories, and `libxmtp` checked out to the correct release commit, run the script:
   `./gen_kotlin.sh` within the `bindings_ffi` folder.
3. **(If only testing, you can skip this step**)Run format (cmd + opt + l) function to keep the code format consistent and diff small for `xmtp-android/library/src/main/java/xmtpv3.kt`

You should now be on the latest libxmtp. Tests will fail if the jniLibs do not match the version of xmtpv3.
