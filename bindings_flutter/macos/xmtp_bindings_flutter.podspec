# These files should be identical:
#  - ios/xmtp_bindings_flutter.podspec
#  - macos/xmtp_bindings_flutter.podspec
# See generally
#  https://cjycode.com/flutter_rust_bridge/library/platform_setup/ios_and_macos.html

release_version = '0.0.1-development.2'
# These versions should be kept in sync:
# - pubspec.yaml
# - android/gradle.properties
# - ios/xmtp_bindings_flutter.podspec
# - macos/xmtp_bindings_flutter.podspec
# TODO: automate that ^

library_name = 'xmtp_bindings_flutter'
release_tag = "#{library_name}-v#{release_version}"
framework_name = "#{library_name}.xcframework"
framework_archive_file = "#{framework_name}.zip"

# If the artifacts don't exist then we download it for this release from GitHub.
# Note: for now we just ship the artifacts (see `setup-***-assets.sh`)
# TODO: actually download them from github releases etc.
url = "https://github.com/xmtp/libxmtp/releases/download/#{release_tag}/#{framework_archive_file}"

# Setup the xmtp_bindings_flutter.xcframework for the pod.
`../scripts/setup-apple-artifacts.sh #{framework_name} #{framework_archive_file} #{url}`
exit $?.exitstatus unless $?.success?

Pod::Spec.new do |spec|
  spec.name          = library_name
  spec.version       = release_version
  spec.homepage      = 'https://github.com/xmtp/libxmtp'
  spec.authors       = { 'XMTP Engineering' => 'eng@xmtp.com' }
  spec.summary       = 'Flutter bindings for libxmtp'

  spec.source              = { :path => '.' }
  spec.source_files        = 'Classes/**/*'
  spec.public_header_files = 'Classes/**/*.h'
  spec.vendored_frameworks = "Frameworks/#{framework_name}"
end