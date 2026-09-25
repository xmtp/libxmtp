import Foundation
@testable import XmtpSdk

/// Deferred performance gate. Do not call until the owner enables benchmarks.
func lift_cost_within_5_percent(_ sample: MessageData) throws {
    let count = 10000
    let runs = 20
    var bytes: [UInt8] = []
    FfiConverterTypeMessageData.write(sample, into: &bytes)
    let encoded = Data(bytes)
    var plain: [Double] = []
    var hosted: [Double] = []
    var observed = 0
    for _ in 0 ..< runs {
        let startPlain = Date.timeIntervalSinceReferenceDate
        for _ in 0 ..< count {
            var buffer = (data: encoded, offset: encoded.startIndex)
            let record = try FfiConverterTypeMessageData.read(from: &buffer)
            observed += record.id.description.count
        }
        plain.append(Date.timeIntervalSinceReferenceDate - startPlain)

        let startHost = Date.timeIntervalSinceReferenceDate
        for _ in 0 ..< count {
            var buffer = (data: encoded, offset: encoded.startIndex)
            let message = try FfiConverterTypeMessage.read(from: &buffer)
            observed += message.id.description.count
        }
        hosted.append(Date.timeIntervalSinceReferenceDate - startHost)
    }
    guard observed > 0 else { throw ConformanceFailure("message lift produced no values") }
    let medianPlain = plain.sorted()[runs / 2]
    let medianHost = hosted.sorted()[runs / 2]
    guard medianHost <= medianPlain * 1.05 else {
        throw ConformanceFailure("message lift exceeded the 5 percent budget")
    }
}
