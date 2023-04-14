public func query_topic<GenericIntoRustString: IntoRustString>(_ topic: GenericIntoRustString) async -> ResponseJson {
    func onComplete(cbWrapperPtr: UnsafeMutableRawPointer?, rustFnRetVal: __swift_bridge__$ResponseJson) {
        let wrapper = Unmanaged<CbWrapper$query_topic>.fromOpaque(cbWrapperPtr!).takeRetainedValue()
        wrapper.cb(.success(rustFnRetVal.intoSwiftRepr()))
    }

    return await withCheckedContinuation({ (continuation: CheckedContinuation<ResponseJson, Never>) in
        let callback = { rustFnRetVal in
            continuation.resume(with: rustFnRetVal)
        }

        let wrapper = CbWrapper$query_topic(cb: callback)
        let wrapperPtr = Unmanaged.passRetained(wrapper).toOpaque()

        __swift_bridge__$query_topic(wrapperPtr, onComplete, { let rustString = topic.intoRustString(); rustString.isOwned = false; return rustString.ptr }())
    })
}
class CbWrapper$query_topic {
    var cb: (Result<ResponseJson, Never>) -> ()

    public init(cb: @escaping (Result<ResponseJson, Never>) -> ()) {
        self.cb = cb
    }
}
public struct ResponseJson {
    public var error: RustString
    public var json: RustString

    public init(error: RustString,json: RustString) {
        self.error = error
        self.json = json
    }

    @inline(__always)
    func intoFfiRepr() -> __swift_bridge__$ResponseJson {
        { let val = self; return __swift_bridge__$ResponseJson(error: { let rustString = val.error.intoRustString(); rustString.isOwned = false; return rustString.ptr }(), json: { let rustString = val.json.intoRustString(); rustString.isOwned = false; return rustString.ptr }()); }()
    }
}
extension __swift_bridge__$ResponseJson {
    @inline(__always)
    func intoSwiftRepr() -> ResponseJson {
        { let val = self; return ResponseJson(error: RustString(ptr: val.error), json: RustString(ptr: val.json)); }()
    }
}
extension __swift_bridge__$Option$ResponseJson {
    @inline(__always)
    func intoSwiftRepr() -> Optional<ResponseJson> {
        if self.is_some {
            return self.val.intoSwiftRepr()
        } else {
            return nil
        }
    }

    @inline(__always)
    static func fromSwiftRepr(_ val: Optional<ResponseJson>) -> __swift_bridge__$Option$ResponseJson {
        if let v = val {
            return __swift_bridge__$Option$ResponseJson(is_some: true, val: v.intoFfiRepr())
        } else {
            return __swift_bridge__$Option$ResponseJson(is_some: false, val: __swift_bridge__$ResponseJson())
        }
    }
}


