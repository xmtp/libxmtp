require 'json'
package = JSON.parse(File.read(File.join(__dir__, 'package.json')))

Pod::Spec.new do |s|
  s.name = 'XmtpReactNative'
  s.version = package['version']
  s.summary = 'XMTP native SDK for React Native'
  s.homepage = 'https://github.com/xmtp/libxmtp'
  s.license = { :type => 'MIT' }
  s.author = { 'XMTP' => 'https://xmtp.org' }
  s.source = { :git => 'https://github.com/xmtp/libxmtp.git', :tag => "react-native-sdk-#{s.version}" }
  s.platforms = { :ios => '15.1' }
  s.source_files = 'ios/**/*.{h,mm}', 'cpp/**/*.{h,hpp,cpp}'
  s.private_header_files = 'cpp/**/*.{h,hpp}'
  s.vendored_frameworks = 'ios/XmtpSdkFFI.xcframework'
  s.libraries = 'c++', 'resolv'
  s.frameworks = 'Security', 'SystemConfiguration'
  s.pod_target_xcconfig = {
    'CLANG_CXX_LANGUAGE_STANDARD' => 'c++20',
    'HEADER_SEARCH_PATHS' => '$(inherited) "${PODS_TARGET_SRCROOT}/cpp" "${PODS_TARGET_SRCROOT}/cpp/generated" "${PODS_TARGET_SRCROOT}/cpp/runtime"',
  }
  install_modules_dependencies(s)
end
