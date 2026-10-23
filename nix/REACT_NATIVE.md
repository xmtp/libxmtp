# React Native Nix builds

RN runtime sources (`ubjs-react-native`) come from the same fork as bindgen.
The RN render has its own `sdkProvenance.render.react-native` identity. Native
adapters and package tools stay outside the Rust compile closure. The JS/types
product `xmtp-sdk-react-native-js` has no Android/iOS binary dependency; docs
use it directly.

`dev/nix-shell --shell react-native 'COMMAND'` selects the locked RN/Expo
Android tools, Gradle, JDK, and CocoaPods. It does not change sibling pins.
`lib.reactNativeXcode` and `lib/react-native-env.nix` own RN's separate exact
Apple pin. Keep the sibling Xcode in `ios-env.nix`. Each canonical derivation
selects its own pin, so a cold component build needs both installations but
needs no global Xcode switch or separate native-base build.

Apple SDK compiles remap transient build roots through the stable compiler
wrappers in `lib/apple-native-tool.sh`. Dependency and final phases use the
same wrapper paths and caller flags, so remapping does not invalidate Cargo's
dependency fingerprints. The C wrapper also stabilizes OpenSSL's configured
engine/module directory macros before compilation. It does not rewrite
sources or completed artifacts. Exact NAR and signature checks still admit
only the tested bytes; remapping alone is not proof of byte determinism.

The shared CI installer admits exact read-only host inputs through
`extra-allowed-impure-host-deps`: the pinned Xcode Contents directory,
`/usr/bin/xcrun`, `/usr/bin/ditto`, `/usr/bin/env`, `/usr/bin/touch`, the existing Xcode license record at
`/Library/Preferences/com.apple.dt.Xcode.plist`, and `/usr/share/firmlinks`.
The launcher checks the license record; Xcode SDK discovery reads firmlinks.
Stock Expo header phases use `ditto`; their unchanged shell scripts use `env`
to select the declared Nix Bash or Node from the existing build PATH. Xcode uses
its hardcoded `touch` executable to finalize frameworks and app bundles.
Simulator and device plug-ins also load the exact CoreSimulator, DVTDownloads,
ROCKit, CoreDevice, and CoreDeviceUtilities bundles under
`/Library/Developer/PrivateFrameworks`, and MobileDevice and Mercury under
`/Library/Apple/System/Library/PrivateFrameworks`. MobileDevice is the real
target of its `/System/Library/PrivateFrameworks` symlink. These are bundle
reads only; they do not grant service or network access.
These inputs do not accept a license or change host preferences.

Only the Expo iOS Nix consumer selects the guarded CocoaPods adapter. After a
successful real install, it atomically adapts the exact locked ExpoModulesJSI
58.0.15 build script. The nested SwiftPM build uses an owned home and manifest
module cache, the same no-signing intent as the app, and a process-local setting
that disables only SwiftPM's additional manifest sandbox. The outer Nix sandbox
and network denial stay active. The dependency script remains unchanged in the
lockfile and content store; no global defaults are written. After verifying the
locked Expo macro package and executable, the adapter also appends the pinned
Swift compiler's `-Xfrontend -disable-sandbox` only to generated `OTHER_SWIFT_FLAGS`
lines that load that exact macro. This disables the compiler's additional sandbox
for subprocesses in those invocations, including loaded Xcode plug-ins; the outer
Nix sandbox still applies. Existing inherited and module-map flags are preserved.
Only the Expo iOS consumer exports the locked CLI's shell-only
`__UNSAFE_EXPO_HOME_DIRECTORY` override to `$TMPDIR/expo-cli-home`. Xcode can
replace `HOME` with the build user's passwd home; the CLI override keeps its
anonymous ID, settings, and cache writes in the owned build directory without
changing `HOME`, project dotenv files, or host permissions.
The Darwin native
dependency farm retains this adapter and its declared tools.
The two iOS consumer app derivations also allow only the read-only LaunchServices
`com.apple.lsd.mapdb` lookup. It reads installed host type-registration metadata
so Xcode can recognize workspace types. Nix accepts
this additive rule only with command-local `sandbox=relaxed`; these app roots
set `__noChroot=false` and remain sandboxed, including network denial. This is
not a global setting. Native/base/component and fixed-output dependency modes
retain their existing behavior.

Import the checked canonical package before building consumers. All four
reachable native `__noChroot=true` derivations are behind that already-realized
package output, so the consumer step does not build them. CI and release select
the mode only for the Darwin consumer command. Trusted warming builds consumers
in the post-merge RN cache job, then builds `ciSets.<system>.rn` with the same
mode. The `dev` and `release` sets contain no consumer, so they need no relaxed
sandbox mode. Read [CI isolation rules](../.github/CI.md#react-native-device-chain)
before changing warm sets or job dependencies.

On a local trusted Nix installation,
a canonical consumer build can use the same narrow per-command options:
`dev/nix-shell 'nix build --option sandbox relaxed --option extra-allowed-impure-host-deps "/Applications/Xcode_27.0.app/Contents /usr/bin/xcrun /usr/bin/ditto /usr/bin/env /usr/bin/touch /Library/Preferences/com.apple.dt.Xcode.plist /usr/share/firmlinks /Library/Developer/PrivateFrameworks/CoreSimulator.framework /Library/Developer/PrivateFrameworks/DVTDownloads.framework /Library/Developer/PrivateFrameworks/ROCKit.framework /Library/Developer/PrivateFrameworks/CoreDevice.framework /Library/Developer/PrivateFrameworks/CoreDeviceUtilities.framework /Library/Apple/System/Library/PrivateFrameworks/MobileDevice.framework /Library/Apple/System/Library/PrivateFrameworks/Mercury.framework" --no-link .#react-native-consumer-ios'`.
An untrusted daemon user must ask its administrator to admit those exact paths.
Keep the sandbox enabled; do not allow all of `/Applications` or `/usr/bin`. The local shell
route below does not build the canonical consumer derivation.

For local Apple proofs, use `dev/nix-shell --shell react-native-local 'COMMAND'`.
This opt-in shell resolves the selected host Xcode and exports its full identity
with `XMTP_RN_XCODE_MODE=local`. Canonical shells and app derivations require
the exact versioned Xcode path and export mode `canonical`. Both remove Nix's
`SDKROOT` override and route Apple tools to the real Xcode. Local receipts
must keep their local identity; they do not satisfy canonical CI admission.

`react-native-js-tools-deps` retains the four exact compiler/declaration
archives as a narrow warm-set member. Keep it in each producer stage and a warm
set (`dev` on Linux, `rn` on Darwin) so empty-store closure checks cover these
build-time inputs.

RN Gradle runs on JDK 21. Its included settings plugin compiles with the
declared JDK 17. The Gradle installation properties name both JDKs and disable
automatic discovery and downloads. Both RN shells select this Gradle through
`XMTP_RN_GRADLE`. Keep its closure in the Android native dependency farm.
The iOS native dependency farm retains the declared `rsync` used by CocoaPods.

Linux builds `xmtp-sdk-react-native-android`; Darwin builds
`xmtp-sdk-react-native-ios`. `ciStages.<system>.components` lists the host's
complete producer outputs. The Linux `xmtp-sdk-react-native-package` declares
both immutable component inputs. Export/import them with
`python3.11 nix/react-native-exchange.py` through the RN recipes. Assembly must
follow both producers; consumer bundles follow assembly. PR exports stay in
CI jobs. Trusted writers receive only exports from the same main run.
Jobs with a Cachix token skip the exchange; `react-native-exchange.py roots`
prints a stage's roots. `--kind test` selects
`legacyPackages.<system>.reactNative.testStages`, the stages of the partial
`xmtp-sdk-react-native-test-package`; `--kind test-android` selects
`androidTestStages`, the Linux-only stages of
`xmtp-sdk-react-native-test-package-android` (the Android smoke).

Check export admission with
`dev/nix-shell 'python3.11 -B nix/lib/test_react_native_exchange.py'`.

Expo's published native build scripts use `shasum`; the iOS consumer and RN
Darwin shells declare Nix Perl for it. Keep Perl and rsync in the iOS native
dependency farm so post-merge warming retains these build tools.

RN CocoaPods probes spawn `command -v ccache` directly through Ruby. The
`react-native-command.nix` store tool delegates to the pinned shell builtin.
Keep it in the iOS consumer, Darwin RN shells, and iOS warm tool farm; it
does not require host `/usr/bin/command` or enable ccache.
