Pod::Spec.new do |s|
  s.name           = 'MessengerPlatform'
  s.version        = '1.0.0'
  s.summary        = 'Platform helpers for the XMTP React Native example'
  s.description    = 'Launch environment, file protection, backup exclusion and the navigation subtitle.'
  s.license        = 'MIT'
  s.author         = 'XMTP Labs'
  s.homepage       = 'https://github.com/xmtp/libxmtp/tree/main/apps/example-react-native'
  s.platforms      = { :ios => '26.0' }
  s.swift_version  = '5.9'
  s.source         = { git: 'https://github.com/xmtp/libxmtp.git' }
  s.static_framework = true

  s.dependency 'ExpoModulesCore'

  s.source_files = '**/*.swift'
  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    'SWIFT_COMPILATION_MODE' => 'wholemodule'
  }
end
