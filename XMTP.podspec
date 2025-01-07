Pod::Spec.new do |spec|
  spec.name         = "XMTP"
  spec.version      = "3.0.22"

  spec.summary      = "XMTP SDK Cocoapod"

  spec.description  = <<-DESC
  The XMTP cocoapod implements the XMTP protocol for iOS. It handles cryptographic operations and network communication with the XMTP network.
                   DESC

  spec.homepage     	= "https://github.com/xmtp/xmtp-ios"

  spec.license      	= "MIT"
  spec.author       	= { "XMTP" => "eng@xmtp.com" }

  spec.platform      	= :ios, '14.0', :macos, '11.0'

  spec.swift_version  = '5.3'

  spec.source       	= { :git => "https://github.com/xmtp/xmtp-ios.git", :tag => "#{spec.version}" }
  spec.source_files  	= "Sources/**/*.swift"
  spec.frameworks 		= "CryptoKit", "UIKit"

  spec.dependency 'CSecp256k1', '~> 0.2'
  spec.dependency "Connect-Swift", "= 1.0.0"
  spec.dependency 'LibXMTP', '= 3.0.18'
  spec.dependency 'CryptoSwift', '= 1.8.3'
  spec.dependency 'SQLCipher', '= 4.5.7'
  
  spec.ios.deployment_target = '14.0'
end
