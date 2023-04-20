public func create_client<GenericIntoRustString: IntoRustString>(_ host: GenericIntoRustString, _ is_secure: Bool) async throws -> RustClient {
    func onComplete(cbWrapperPtr: UnsafeMutableRawPointer?, rustFnRetVal: __private__ResultPtrAndPtr) {
        let wrapper = Unmanaged<CbWrapper$create_client>.fromOpaque(cbWrapperPtr!).takeRetainedValue()
        if rustFnRetVal.is_ok {
            wrapper.cb(.success(RustClient(ptr: rustFnRetVal.ok_or_err!)))
        } else {
            wrapper.cb(.failure(RustString(ptr: rustFnRetVal.ok_or_err!)))
        }
    }

    return try await withCheckedThrowingContinuation({ (continuation: CheckedContinuation<RustClient, Error>) in
        let callback = { rustFnRetVal in
            continuation.resume(with: rustFnRetVal)
        }

        let wrapper = CbWrapper$create_client(cb: callback)
        let wrapperPtr = Unmanaged.passRetained(wrapper).toOpaque()

        __swift_bridge__$create_client(wrapperPtr, onComplete, { let rustString = host.intoRustString(); rustString.isOwned = false; return rustString.ptr }(), is_secure)
    })
}
class CbWrapper$create_client {
    var cb: (Result<RustClient, Error>) -> ()

    public init(cb: @escaping (Result<RustClient, Error>) -> ()) {
        self.cb = cb
    }
}
public enum SortDirection {
    case Unspecified
    case Ascending
    case Descending
}
extension SortDirection {
    func intoFfiRepr() -> __swift_bridge__$SortDirection {
        switch self {
            case SortDirection.Unspecified:
                return __swift_bridge__$SortDirection(tag: __swift_bridge__$SortDirection$Unspecified)
            case SortDirection.Ascending:
                return __swift_bridge__$SortDirection(tag: __swift_bridge__$SortDirection$Ascending)
            case SortDirection.Descending:
                return __swift_bridge__$SortDirection(tag: __swift_bridge__$SortDirection$Descending)
        }
    }
}
extension __swift_bridge__$SortDirection {
    func intoSwiftRepr() -> SortDirection {
        switch self.tag {
            case __swift_bridge__$SortDirection$Unspecified:
                return SortDirection.Unspecified
            case __swift_bridge__$SortDirection$Ascending:
                return SortDirection.Ascending
            case __swift_bridge__$SortDirection$Descending:
                return SortDirection.Descending
            default:
                fatalError("Unreachable")
        }
    }
}
extension __swift_bridge__$Option$SortDirection {
    @inline(__always)
    func intoSwiftRepr() -> Optional<SortDirection> {
        if self.is_some {
            return self.val.intoSwiftRepr()
        } else {
            return nil
        }
    }
    @inline(__always)
    static func fromSwiftRepr(_ val: Optional<SortDirection>) -> __swift_bridge__$Option$SortDirection {
        if let v = val {
            return __swift_bridge__$Option$SortDirection(is_some: true, val: v.intoFfiRepr())
        } else {
            return __swift_bridge__$Option$SortDirection(is_some: false, val: __swift_bridge__$SortDirection())
        }
    }
}
extension SortDirection: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_SortDirection$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_SortDirection$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: Self) {
        __swift_bridge__$Vec_SortDirection$push(vecPtr, value.intoFfiRepr())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let maybeEnum = __swift_bridge__$Vec_SortDirection$pop(vecPtr)
        return maybeEnum.intoSwiftRepr()
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<Self> {
        let maybeEnum = __swift_bridge__$Vec_SortDirection$get(vecPtr, index)
        return maybeEnum.intoSwiftRepr()
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<Self> {
        let maybeEnum = __swift_bridge__$Vec_SortDirection$get_mut(vecPtr, index)
        return maybeEnum.intoSwiftRepr()
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_SortDirection$len(vecPtr)
    }
}
public struct IndexCursor {
    public var digest: RustVec<UInt8>
    public var sender_time_ns: UInt64

    public init(digest: RustVec<UInt8>,sender_time_ns: UInt64) {
        self.digest = digest
        self.sender_time_ns = sender_time_ns
    }

    @inline(__always)
    func intoFfiRepr() -> __swift_bridge__$IndexCursor {
        { let val = self; return __swift_bridge__$IndexCursor(digest: { let val = val.digest; val.isOwned = false; return val.ptr }(), sender_time_ns: val.sender_time_ns); }()
    }
}
extension __swift_bridge__$IndexCursor {
    @inline(__always)
    func intoSwiftRepr() -> IndexCursor {
        { let val = self; return IndexCursor(digest: RustVec(ptr: val.digest), sender_time_ns: val.sender_time_ns); }()
    }
}
extension __swift_bridge__$Option$IndexCursor {
    @inline(__always)
    func intoSwiftRepr() -> Optional<IndexCursor> {
        if self.is_some {
            return self.val.intoSwiftRepr()
        } else {
            return nil
        }
    }

    @inline(__always)
    static func fromSwiftRepr(_ val: Optional<IndexCursor>) -> __swift_bridge__$Option$IndexCursor {
        if let v = val {
            return __swift_bridge__$Option$IndexCursor(is_some: true, val: v.intoFfiRepr())
        } else {
            return __swift_bridge__$Option$IndexCursor(is_some: false, val: __swift_bridge__$IndexCursor())
        }
    }
}
public struct PagingInfo {
    public var limit: UInt32
    public var cursor: Optional<IndexCursor>
    public var direction: SortDirection

    public init(limit: UInt32,cursor: Optional<IndexCursor>,direction: SortDirection) {
        self.limit = limit
        self.cursor = cursor
        self.direction = direction
    }

    @inline(__always)
    func intoFfiRepr() -> __swift_bridge__$PagingInfo {
        { let val = self; return __swift_bridge__$PagingInfo(limit: val.limit, cursor: __swift_bridge__$Option$IndexCursor.fromSwiftRepr(val.cursor), direction: val.direction.intoFfiRepr()); }()
    }
}
extension __swift_bridge__$PagingInfo {
    @inline(__always)
    func intoSwiftRepr() -> PagingInfo {
        { let val = self; return PagingInfo(limit: val.limit, cursor: val.cursor.intoSwiftRepr(), direction: val.direction.intoSwiftRepr()); }()
    }
}
extension __swift_bridge__$Option$PagingInfo {
    @inline(__always)
    func intoSwiftRepr() -> Optional<PagingInfo> {
        if self.is_some {
            return self.val.intoSwiftRepr()
        } else {
            return nil
        }
    }

    @inline(__always)
    static func fromSwiftRepr(_ val: Optional<PagingInfo>) -> __swift_bridge__$Option$PagingInfo {
        if let v = val {
            return __swift_bridge__$Option$PagingInfo(is_some: true, val: v.intoFfiRepr())
        } else {
            return __swift_bridge__$Option$PagingInfo(is_some: false, val: __swift_bridge__$PagingInfo())
        }
    }
}
public struct Envelope {
    public var content_topic: RustString
    public var timestamp_ns: UInt64
    public var message: RustVec<UInt8>

    public init(content_topic: RustString,timestamp_ns: UInt64,message: RustVec<UInt8>) {
        self.content_topic = content_topic
        self.timestamp_ns = timestamp_ns
        self.message = message
    }

    @inline(__always)
    func intoFfiRepr() -> __swift_bridge__$Envelope {
        { let val = self; return __swift_bridge__$Envelope(content_topic: { let rustString = val.content_topic.intoRustString(); rustString.isOwned = false; return rustString.ptr }(), timestamp_ns: val.timestamp_ns, message: { let val = val.message; val.isOwned = false; return val.ptr }()); }()
    }
}
extension __swift_bridge__$Envelope {
    @inline(__always)
    func intoSwiftRepr() -> Envelope {
        { let val = self; return Envelope(content_topic: RustString(ptr: val.content_topic), timestamp_ns: val.timestamp_ns, message: RustVec(ptr: val.message)); }()
    }
}
extension __swift_bridge__$Option$Envelope {
    @inline(__always)
    func intoSwiftRepr() -> Optional<Envelope> {
        if self.is_some {
            return self.val.intoSwiftRepr()
        } else {
            return nil
        }
    }

    @inline(__always)
    static func fromSwiftRepr(_ val: Optional<Envelope>) -> __swift_bridge__$Option$Envelope {
        if let v = val {
            return __swift_bridge__$Option$Envelope(is_some: true, val: v.intoFfiRepr())
        } else {
            return __swift_bridge__$Option$Envelope(is_some: false, val: __swift_bridge__$Envelope())
        }
    }
}

public class RustSubscription: RustSubscriptionRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$RustSubscription$_free(ptr)
        }
    }
}
public class RustSubscriptionRefMut: RustSubscriptionRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
extension RustSubscriptionRefMut {
    public func close() {
        __swift_bridge__$RustSubscription$close(ptr)
    }
}
public class RustSubscriptionRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension RustSubscriptionRef {
    public func get_messages() throws -> RustVec<Envelope> {
        try { let val = __swift_bridge__$RustSubscription$get_messages(ptr); if val.is_ok { return RustVec(ptr: val.ok_or_err!) } else { throw RustString(ptr: val.ok_or_err!) } }()
    }
}
extension RustSubscription: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_RustSubscription$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_RustSubscription$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: RustSubscription) {
        __swift_bridge__$Vec_RustSubscription$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_RustSubscription$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (RustSubscription(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<RustSubscriptionRef> {
        let pointer = __swift_bridge__$Vec_RustSubscription$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return RustSubscriptionRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<RustSubscriptionRefMut> {
        let pointer = __swift_bridge__$Vec_RustSubscription$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return RustSubscriptionRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_RustSubscription$len(vecPtr)
    }
}


public class QueryResponse: QueryResponseRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$QueryResponse$_free(ptr)
        }
    }
}
extension QueryResponse {
    public func envelopes() -> RustVec<Envelope> {
        RustVec(ptr: __swift_bridge__$QueryResponse$envelopes({isOwned = false; return ptr;}()))
    }

    public func paging_info() -> Optional<PagingInfo> {
        __swift_bridge__$QueryResponse$paging_info({isOwned = false; return ptr;}()).intoSwiftRepr()
    }
}
public class QueryResponseRefMut: QueryResponseRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class QueryResponseRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension QueryResponse: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_QueryResponse$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_QueryResponse$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: QueryResponse) {
        __swift_bridge__$Vec_QueryResponse$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_QueryResponse$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (QueryResponse(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<QueryResponseRef> {
        let pointer = __swift_bridge__$Vec_QueryResponse$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return QueryResponseRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<QueryResponseRefMut> {
        let pointer = __swift_bridge__$Vec_QueryResponse$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return QueryResponseRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_QueryResponse$len(vecPtr)
    }
}


public class RustClient: RustClientRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$RustClient$_free(ptr)
        }
    }
}
public class RustClientRefMut: RustClientRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
extension RustClientRefMut {
    public func query<GenericIntoRustString: IntoRustString>(_ topic: GenericIntoRustString, _ start_time_ns: Optional<UInt64>, _ end_time_ns: Optional<UInt64>, _ paging_info: Optional<PagingInfo>) async throws -> QueryResponse {
        func onComplete(cbWrapperPtr: UnsafeMutableRawPointer?, rustFnRetVal: __private__ResultPtrAndPtr) {
            let wrapper = Unmanaged<CbWrapper$RustClient$query>.fromOpaque(cbWrapperPtr!).takeRetainedValue()
            if rustFnRetVal.is_ok {
                wrapper.cb(.success(QueryResponse(ptr: rustFnRetVal.ok_or_err!)))
            } else {
                wrapper.cb(.failure(RustString(ptr: rustFnRetVal.ok_or_err!)))
            }
        }

        return try await withCheckedThrowingContinuation({ (continuation: CheckedContinuation<QueryResponse, Error>) in
            let callback = { rustFnRetVal in
                continuation.resume(with: rustFnRetVal)
            }

            let wrapper = CbWrapper$RustClient$query(cb: callback)
            let wrapperPtr = Unmanaged.passRetained(wrapper).toOpaque()

            __swift_bridge__$RustClient$query(wrapperPtr, onComplete, ptr, { let rustString = topic.intoRustString(); rustString.isOwned = false; return rustString.ptr }(), { let val = start_time_ns; return __private__OptionU64(val: val ?? 123, is_some: val != nil); }(), { let val = end_time_ns; return __private__OptionU64(val: val ?? 123, is_some: val != nil); }(), __swift_bridge__$Option$PagingInfo.fromSwiftRepr(paging_info))
        })
    }
    class CbWrapper$RustClient$query {
        var cb: (Result<QueryResponse, Error>) -> ()
    
        public init(cb: @escaping (Result<QueryResponse, Error>) -> ()) {
            self.cb = cb
        }
    }

    public func publish<GenericIntoRustString: IntoRustString>(_ token: GenericIntoRustString, _ envelopes: RustVec<Envelope>) async throws -> () {
        func onComplete(cbWrapperPtr: UnsafeMutableRawPointer?, rustFnRetVal: __private__ResultPtrAndPtr) {
            let wrapper = Unmanaged<CbWrapper$RustClient$publish>.fromOpaque(cbWrapperPtr!).takeRetainedValue()
            if rustFnRetVal.is_ok {
                wrapper.cb(.success(()(ptr: rustFnRetVal.ok_or_err!)))
            } else {
                wrapper.cb(.failure(RustString(ptr: rustFnRetVal.ok_or_err!)))
            }
        }

        return try await withCheckedThrowingContinuation({ (continuation: CheckedContinuation<(), Error>) in
            let callback = { rustFnRetVal in
                continuation.resume(with: rustFnRetVal)
            }

            let wrapper = CbWrapper$RustClient$publish(cb: callback)
            let wrapperPtr = Unmanaged.passRetained(wrapper).toOpaque()

            __swift_bridge__$RustClient$publish(wrapperPtr, onComplete, ptr, { let rustString = token.intoRustString(); rustString.isOwned = false; return rustString.ptr }(), { let val = envelopes; val.isOwned = false; return val.ptr }())
        })
    }
    class CbWrapper$RustClient$publish {
        var cb: (Result<(), Error>) -> ()
    
        public init(cb: @escaping (Result<(), Error>) -> ()) {
            self.cb = cb
        }
    }

    public func subscribe<GenericIntoRustString: IntoRustString>(_ topics: RustVec<GenericIntoRustString>) async throws -> RustSubscription {
        func onComplete(cbWrapperPtr: UnsafeMutableRawPointer?, rustFnRetVal: __private__ResultPtrAndPtr) {
            let wrapper = Unmanaged<CbWrapper$RustClient$subscribe>.fromOpaque(cbWrapperPtr!).takeRetainedValue()
            if rustFnRetVal.is_ok {
                wrapper.cb(.success(RustSubscription(ptr: rustFnRetVal.ok_or_err!)))
            } else {
                wrapper.cb(.failure(RustString(ptr: rustFnRetVal.ok_or_err!)))
            }
        }

        return try await withCheckedThrowingContinuation({ (continuation: CheckedContinuation<RustSubscription, Error>) in
            let callback = { rustFnRetVal in
                continuation.resume(with: rustFnRetVal)
            }

            let wrapper = CbWrapper$RustClient$subscribe(cb: callback)
            let wrapperPtr = Unmanaged.passRetained(wrapper).toOpaque()

            __swift_bridge__$RustClient$subscribe(wrapperPtr, onComplete, ptr, { let val = topics; val.isOwned = false; return val.ptr }())
        })
    }
    class CbWrapper$RustClient$subscribe {
        var cb: (Result<RustSubscription, Error>) -> ()
    
        public init(cb: @escaping (Result<RustSubscription, Error>) -> ()) {
            self.cb = cb
        }
    }
}
public class RustClientRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension RustClient: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_RustClient$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_RustClient$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: RustClient) {
        __swift_bridge__$Vec_RustClient$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_RustClient$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (RustClient(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<RustClientRef> {
        let pointer = __swift_bridge__$Vec_RustClient$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return RustClientRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<RustClientRefMut> {
        let pointer = __swift_bridge__$Vec_RustClient$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return RustClientRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_RustClient$len(vecPtr)
    }
}



