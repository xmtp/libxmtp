require 'json'
require 'uri'
Pod::Spec.new do |spec|
  spec.name = 'XMTP'
  spec.module_name = 'XmtpSdk'
  spec.version = "8.0.0"
  spec.summary = 'XMTP messaging SDK'
  spec.description = 'The XMTP SDK uses Rust for messaging, storage, content and attachment transfers.'
  spec.homepage = 'https://github.com/xmtp/libxmtp'
  spec.license = 'MIT'
  spec.author = { 'XMTP' => 'eng@xmtp.com' }
  spec.ios.deployment_target = '14.0'
  spec.osx.deployment_target = '11.0'
  spec.swift_version = '5.0'
  receipt_path = File.join(__dir__, 'ReleaseArtifacts.json')
  if File.exist?(receipt_path)
    begin
      receipt = JSON.parse(File.read(receipt_path))
    rescue JSON::ParserError
      raise ArgumentError, 'Invalid ReleaseArtifacts.json: expected a JSON object with url and sha256.'
    end
    unless receipt.is_a?(Hash) && receipt['url'].is_a?(String) && !receipt['url'].empty? &&
        receipt['sha256'].is_a?(String) && receipt['sha256'].match?(/\A[0-9a-fA-F]{64}\z/)
      raise ArgumentError, 'Invalid ReleaseArtifacts.json: expected a nonempty url and a 64-digit SHA256.'
    end
    begin
      artifact = URI.parse(receipt['url'])
    rescue URI::InvalidURIError
      raise ArgumentError, 'Invalid ReleaseArtifacts.json: expected an XmtpSdkFFI.zip archive URL.'
    end
    unless artifact.is_a?(URI::HTTP) && artifact.host && artifact.path.end_with?('/XmtpSdkFFI.zip')
      raise ArgumentError, 'Invalid ReleaseArtifacts.json: expected an HTTP or HTTPS XmtpSdkFFI.zip archive URL.'
    end
    spec.source = { :http => receipt['url'], :sha256 => receipt['sha256'], :type => :zip }
    spec.vendored_frameworks = 'XmtpSdkFFI.xcframework'
  else
    # A checkout uses the local archive made by just ios build.
    spec.source = { :git => 'https://github.com/xmtp/libxmtp.git', :tag => "ios-#{spec.version}" }
    spec.vendored_frameworks = 'Artifacts/XmtpSdkFFI.xcframework'
  end
  spec.source_files = 'Sources/XmtpSdk/**/*.swift'
  # The native producer supplies an arm64 simulator slice.
  spec.pod_target_xcconfig = { 'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'x86_64' }
  spec.user_target_xcconfig = { 'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'x86_64' }
end
