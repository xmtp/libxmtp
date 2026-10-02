require 'json'
Pod::Spec.new do |spec|
  spec.name = 'XMTP'
  spec.module_name = 'XmtpSdk'
  spec.version = '8.0.0'
  spec.summary = 'XMTP messaging SDK'
  spec.description = 'The XMTP SDK uses Rust for messaging, storage, content and attachment transfers.'
  spec.homepage = 'https://github.com/xmtp/libxmtp'
  spec.license = 'MIT'
  spec.author = { 'XMTP' => 'eng@xmtp.com' }
  spec.ios.deployment_target = '14.0'
  spec.osx.deployment_target = '11.0'
  spec.swift_version = '5.0'
  receipt = JSON.parse(File.read(File.join(__dir__, 'ReleaseArtifacts.json')))
  spec.source = { :http => receipt.fetch('url'), :sha256 => receipt.fetch('sha256'), :type => :zip }
  spec.source_files = 'Sources/XmtpSdk/**/*.swift'
  spec.vendored_frameworks = 'XmtpSdkFFI.xcframework'
end
