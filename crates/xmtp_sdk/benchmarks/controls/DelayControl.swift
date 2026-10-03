@main
struct DelayControl {
    static func main() throws {
        let maximum = UInt64.max / 1_000_000
        let zero = try delayNanoseconds(0)
        let ordinary = try delayNanoseconds(25)
        let largest = try delayNanoseconds(maximum)
        precondition(zero == 0)
        precondition(ordinary == 25_000_000)
        precondition(largest == maximum * 1_000_000)
        do {
            _ = try delayNanoseconds(maximum + 1)
            fatalError("Delay above the limit must throw")
        } catch let error as BenchFailure {
            precondition(error.message == "Delay exceeds the nanosecond limit")
        }
        print("Swift delay zero, ordinary, maximum and overflow controls passed")
    }
}
