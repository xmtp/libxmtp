# Internal installed consumers

These private fixtures validate the packed SDK. They are not public example apps.
Both use React Native 0.88.0 and React 19.3.0. The Expo fixture uses native Expo
58.0.14 and the 58.0.23 bare template. Expo Go is not a supported test host.
`template-source.json`, the separate frozen pnpm locks, and `native-toolchain.json`
record the exact inputs. `compatibility.json` records candidates separately from
passing installed results.

Run native commands through the `react-native` Nix shell. CI supplies an available
64-bit Android emulator or arm64 iOS simulator and a fresh backend. Select the
same immutable package and its built consumers with:

- `XMTP_RN_PACKAGE_DIR`: canonical package output containing `package.tgz` or the
  exact requested version's tarball. `XMTP_REACT_NATIVE_PACKAGE` selects an exact
  tarball directly.
- `XMTP_RN_EXPECTED_VERSION` and `XMTP_RN_EXPECTED_SOURCE`: requested release
  version and exact SDK source identity. The verifier checks all native assets,
  the fork/runtime contract, and the tarball digest against the consumer receipt.
- `XMTP_RN_CONSUMER_BUNDLES`: output containing `bare/consumer-build.json` and
  `expo/consumer-build.json` plus their applications.
- `XMTP_NODE_SDK_DIR`: staged genuine sibling Node SDK package.
- `XMTP_BACKEND_URL`: this run's live backend URL.
- `XMTP_RN_DEVICE`: optional exact simulator/emulator selector.

`dev/nix-shell --shell react-native --command sdks/react-native/scripts/test-installed ios all --conformance`
launches each app and requires fresh sibling-verified assertion reports. Use
`android` on the Android host. Without `--conformance`, the journey checks the
native boundary, encrypted private storage, DM exchange, end, and reopen. The
conformance run also checks API families, independent legacy history conversion,
callbacks/streams, a real Hermes replacement, and installed React helpers. It
then performs a separate fresh-install cold-background launch and asks the OS to
foreground that same runtime. Both reports must pass. A build alone is not an
installed runtime proof.

The fixture directories contain locks and native overrides. They are not complete
native projects. The cached `react-native-consumer-template-bare` and
`react-native-consumer-template-expo` Nix outputs fetch the published archives at
the URL and SHA512 in `template-source.json`. `prepare-template.py` renames the
native identifiers and public signing key, formats JSON, and applies the small
`native.patch` project additions. `build.py` copies this scaffold into its fresh
work directory, then applies the retained native overrides and test hooks. It
never runs Expo prebuild. Generated projects belong under `target/` or a temporary
directory, which are already ignored. Gradle launchers are omitted because native
builds require the declared Nix Gradle executable.

`build.py` accepts an exact tarball and fresh work/output directories. Nix builders
supply `XMTP_RN_TEMPLATE`; the local route realizes the same Nix output before
preparing its work directory. The harness hash covers the template pin,
preparation script, project patch, native overrides, and retained locks. It does
not hash generated staging directories.

The package install retains the frozen external dependency graph while adding
the verified local SDK tarball. Native builders supply `PNPM_STORE_DIR` and the
following replay inputs:

- iOS: `XMTP_RN_IOS_DOWNLOADS` contains the paths in `ios-downloads.json`. Every
  archive is checked against its declared SHA256. The private curl adapter rejects
  undeclared URLs. The committed Pod lock is used in deployment mode; only a
  verified local SDK version stamp may change its own version/checksum.
- Android: `XMTP_RN_GRADLE` selects pinned Gradle; `XMTP_RN_GRADLE_FLAGS_JSON`
  supplies an array of arguments. `XMTP_RN_GRADLE_REPLAY=1` permits the strict
  declared loopback dependency proxy inside the Nix sandbox instead of Gradle's
  cache-only mode. External undeclared fetches remain denied by that build.

`capture-gradle.py` records actual release configurations through the pinned
nixpkgs TLS recording proxy and writes its native download manifest. The fixture
verification metadata records resolved SHA256 values. Refreshing native locks
requires resolving and reviewing genuine dependency bytes. No fake native
libraries are used to capture or validate the graph.

Release fixtures use the fixed public debug key from both pinned upstream
templates. `template-source.json` records its hash, public password and source.
It is a declared test input and is never a production signing key.

The runtime driver checks the current exact harness hash against both the built
receipt and fresh app report. Reports must name the requested mode and exact
ordered phase witnesses, emitted only after the real phases finish. Assertion
label counts cannot admit a result.

Canonical iOS builds require the exact declared versioned Xcode. A distinct local
proof uses `--shell react-native-local`; both build receipt and runner must agree
on its mode and exact selected Xcode identity. Native commands still run in Nix.
