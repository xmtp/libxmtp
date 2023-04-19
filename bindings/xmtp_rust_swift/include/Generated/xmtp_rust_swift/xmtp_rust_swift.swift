public func query<GenericIntoRustString: IntoRustString>(_ host: GenericIntoRustString, _ topic: GenericIntoRustString, _ json_paging_info: GenericIntoRustString) async -> ResponseJson {
    func onComplete(cbWrapperPtr: UnsafeMutableRawPointer?, rustFnRetVal: __swift_bridge__$ResponseJson) {
        let wrapper = Unmanaged<CbWrapper$query>.fromOpaque(cbWrapperPtr!).takeRetainedValue()
        wrapper.cb(.success(rustFnRetVal.intoSwiftRepr()))
    }

    return await withCheckedContinuation({ (continuation: CheckedContinuation<ResponseJson, Never>) in
        let callback = { rustFnRetVal in
            continuation.resume(with: rustFnRetVal)
        }

        let wrapper = CbWrapper$query(cb: callback)
        let wrapperPtr = Unmanaged.passRetained(wrapper).toOpaque()

        __swift_bridge__$query(wrapperPtr, onComplete, { let rustString = host.intoRustString(); rustString.isOwned = false; return rustString.ptr }(), { let rustString = topic.intoRustString(); rustString.isOwned = false; return rustString.ptr }(), { let rustString = json_paging_info.intoRustString(); rustString.isOwned = false; return rustString.ptr }())
    })
}
class CbWrapper$query {
    var cb: (Result<ResponseJson, Never>) -> ()

    public init(cb: @escaping (Result<ResponseJson, Never>) -> ()) {
        self.cb = cb
    }
}
public func publish<GenericIntoRustString: IntoRustString>(_ host: GenericIntoRustString, _ token: GenericIntoRustString, _ json_envelopes: GenericIntoRustString) async -> ResponseJson {
    func onComplete(cbWrapperPtr: UnsafeMutableRawPointer?, rustFnRetVal: __swift_bridge__$ResponseJson) {
        let wrapper = Unmanaged<CbWrapper$publish>.fromOpaque(cbWrapperPtr!).takeRetainedValue()
        wrapper.cb(.success(rustFnRetVal.intoSwiftRepr()))
    }

    return await withCheckedContinuation({ (continuation: CheckedContinuation<ResponseJson, Never>) in
        let callback = { rustFnRetVal in
            continuation.resume(with: rustFnRetVal)
        }

        let wrapper = CbWrapper$publish(cb: callback)
        let wrapperPtr = Unmanaged.passRetained(wrapper).toOpaque()

        __swift_bridge__$publish(wrapperPtr, onComplete, { let rustString = host.intoRustString(); rustString.isOwned = false; return rustString.ptr }(), { let rustString = token.intoRustString(); rustString.isOwned = false; return rustString.ptr }(), { let rustString = json_envelopes.intoRustString(); rustString.isOwned = false; return rustString.ptr }())
    })
}
class CbWrapper$publish {
    var cb: (Result<ResponseJson, Never>) -> ()

    public init(cb: @escaping (Result<ResponseJson, Never>) -> ()) {
        self.cb = cb
    }
}
public func subscribe<GenericIntoRustString: IntoRustString>(_ host: GenericIntoRustString, _ topics: RustVec<GenericIntoRustString>) async -> ResponseJson {
    func onComplete(cbWrapperPtr: UnsafeMutableRawPointer?, rustFnRetVal: __swift_bridge__$ResponseJson) {
        let wrapper = Unmanaged<CbWrapper$subscribe>.fromOpaque(cbWrapperPtr!).takeRetainedValue()
        wrapper.cb(.success(rustFnRetVal.intoSwiftRepr()))
    }

    return await withCheckedContinuation({ (continuation: CheckedContinuation<ResponseJson, Never>) in
        let callback = { rustFnRetVal in
            continuation.resume(with: rustFnRetVal)
        }

        let wrapper = CbWrapper$subscribe(cb: callback)
        let wrapperPtr = Unmanaged.passRetained(wrapper).toOpaque()

        __swift_bridge__$subscribe(wrapperPtr, onComplete, { let rustString = host.intoRustString(); rustString.isOwned = false; return rustString.ptr }(), { let val = topics; val.isOwned = false; return val.ptr }())
    })
}
class CbWrapper$subscribe {
    var cb: (Result<ResponseJson, Never>) -> ()

    public init(cb: @escaping (Result<ResponseJson, Never>) -> ()) {
        self.cb = cb
    }
}
public func poll_subscription<GenericIntoRustString: IntoRustString>(_ subscription_id: GenericIntoRustString) async -> ResponseJson {
    func onComplete(cbWrapperPtr: UnsafeMutableRawPointer?, rustFnRetVal: __swift_bridge__$ResponseJson) {
        let wrapper = Unmanaged<CbWrapper$poll_subscription>.fromOpaque(cbWrapperPtr!).takeRetainedValue()
        wrapper.cb(.success(rustFnRetVal.intoSwiftRepr()))
    }

    return await withCheckedContinuation({ (continuation: CheckedContinuation<ResponseJson, Never>) in
        let callback = { rustFnRetVal in
            continuation.resume(with: rustFnRetVal)
        }

        let wrapper = CbWrapper$poll_subscription(cb: callback)
        let wrapperPtr = Unmanaged.passRetained(wrapper).toOpaque()

        __swift_bridge__$poll_subscription(wrapperPtr, onComplete, { let rustString = subscription_id.intoRustString(); rustString.isOwned = false; return rustString.ptr }())
    })
}
class CbWrapper$poll_subscription {
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


