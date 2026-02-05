Pod::Spec.new do |spec|
  spec.name         = "XMTP"
  spec.version      = "4.9.0-dev.50f0f4a"

  spec.summary      = "XMTP SDK Cocoapod"

  spec.description  = <<-DESC
  The XMTP cocoapod implements the XMTP protocol for iOS. It handles cryptographic operations and network communication with the XMTP network.
                   DESC

  spec.homepage     	= "https://github.com/xmtp/libxmtp"

  spec.license      	= "MIT"
  spec.author       	= { "XMTP" => "eng@xmtp.com" }

  spec.platform      	= :ios, '14.0', :macos, '11.0'

  spec.swift_version  = '6.1'

  # Release archive contains libxmtp uniffi bindings Sources/** and LibXMTPSwiftFFI.xcframework
  spec.source       	= { :http => "https://github.com/xmtp/libxmtp/releases/download/ios-#{spec.version}/XMTP-#{spec.version}.zip", :type => :zip }
  spec.source_files  	= "Sources/**/*.swift"
  spec.frameworks 		= "CryptoKit", "UIKit"

  spec.dependency "Connect-Swift", "~> 1.2.0"
  spec.dependency 'CryptoSwift', '= 1.8.3'
  spec.dependency 'SQLCipher', '= 4.5.7'
  spec.vendored_frameworks = 'LibXMTPSwiftFFI.xcframework'

  spec.ios.deployment_target = '14.0'
end
