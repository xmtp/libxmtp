require 'cocoapods-core'
require 'json'
require 'tmpdir'
require 'digest'

source = File.read(ARGV.fetch(0, File.expand_path('../XMTP.podspec', __dir__)))
mode = ARGV.fetch(1, 'all')
def check(condition, name)
  raise "FAIL: #{name}" unless condition
  puts "PASS: #{name}"
end

def evaluate(source, receipt = nil)
  Dir.mktmpdir('xmtp-podspec-control') do |directory|
    path = File.join(directory, 'XMTP.podspec')
    File.write(path, source)
    File.write(File.join(directory, 'ReleaseArtifacts.json'), receipt) if receipt
    yield Pod::Specification.from_file(path)
  end
end

if mode == 'all' || mode == 'checkout'
  evaluate(source) do |spec|
    check(spec.version.to_s == '8.0.0', 'version retained')
    check(spec.source == { :git => 'https://github.com/xmtp/libxmtp.git', :tag => 'ios-8.0.0' }, 'checkout git/tag template without receipt')
    check(spec.attributes_hash['vendored_frameworks'] == 'Artifacts/XmtpSdkFFI.xcframework', 'checkout local archive path')
  end
end
fixture = { 'url' => 'https://example.invalid/fixture/XmtpSdkFFI.zip', 'sha256' => 'a' * 64 }
if mode == 'all' || mode == 'architecture'
  evaluate(source, fixture.to_json) do |spec|
    check(spec.source == { :http => fixture['url'], :sha256 => fixture['sha256'], :type => :zip }, 'release HTTP and supplied hash unchanged')
    check(spec.attributes_hash['vendored_frameworks'] == 'XmtpSdkFFI.xcframework', 'release archive root path')
    check(spec.attributes_hash['source_files'] == 'Sources/XmtpSdk/**/*.swift', 'release Swift source path')
    %w[pod_target_xcconfig user_target_xcconfig].each do |key|
      check(spec.attributes_hash[key]['EXCLUDED_ARCHS[sdk=iphonesimulator*]'] == 'x86_64', "#{key} matches producer simulator architectures")
      check(spec.attributes_hash[key]['EXCLUDED_ARCHS[sdk=macosx*]'] == 'x86_64', "#{key} matches producer macOS architectures")
    end
    check(spec.deployment_target(:ios) == '14.0', 'iOS floor retained')
    check(spec.deployment_target(:osx) == '11.0', 'macOS floor retained')
  end
end
if mode == 'all'
  invalid_receipts = ['not json', '[]', '{}', { 'url' => fixture['url'] }.to_json]
  invalid_receipts << { 'url' => fixture['url'], 'sha256' => 'wrong' }.to_json
  ['not a URL', 'https://example.invalid/Other.zip'].each do |url|
    invalid_receipts << { 'url' => url, 'sha256' => fixture['sha256'] }.to_json
  end
  invalid_receipts.each do |receipt|
    failed = false
    begin
      evaluate(source, receipt) { raise 'An invalid receipt was accepted' }
    rescue Pod::DSLError => error
      failed = error.message.include?('Invalid ReleaseArtifacts.json')
    end
    check(failed, 'invalid or partial receipt fails; no checkout fallback')
  end
end
puts "CocoaPods core #{Gem.loaded_specs.fetch('cocoapods-core').version}; source SHA256 #{Digest::SHA256.hexdigest(source)}"
