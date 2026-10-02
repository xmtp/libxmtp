open class EventReader: EventReaderProtocol, @unchecked Sendable {

open func end()async throws   {
    return
        try  await uniffiRustCallAsync(
            rustFutureFunc: {
                uniffi_xmtp_sdk_fn_method_eventreader_end(
                        self.uniffiCloneHandle()
                )
            },
            pollFunc: ffi_xmtp_sdk_rust_future_poll_void,
            completeFunc: ffi_xmtp_sdk_rust_future_complete_void,
            freeFunc: ffi_xmtp_sdk_rust_future_free_void,
            liftFunc: { $0 },
            errorHandler: FfiConverterTypeXmtpError_lift
        )
}


open func next()async throws  -> ClientEvent?  {
    return
        try  await uniffiRustCallAsync(
            rustFutureFunc: {
                uniffi_xmtp_sdk_fn_method_eventreader_next(
                        self.uniffiCloneHandle()
                )
            },
            pollFunc: ffi_xmtp_sdk_rust_future_poll_rust_buffer,
            completeFunc: ffi_xmtp_sdk_rust_future_complete_rust_buffer,
            freeFunc: ffi_xmtp_sdk_rust_future_free_rust_buffer,
            liftFunc: FfiConverterOptionTypeClientEvent.lift,
            errorHandler: FfiConverterTypeXmtpError_lift
        )
}

}

open class Client: ClientProtocol, @unchecked Sendable {

open func end()async throws   {
    return
        try  await uniffiRustCallAsync(
            rustFutureFunc: {
                uniffi_xmtp_sdk_fn_method_client_end(
                        self.uniffiCloneHandle()
                )
            },
            pollFunc: ffi_xmtp_sdk_rust_future_poll_void,
            completeFunc: ffi_xmtp_sdk_rust_future_complete_void,
            freeFunc: ffi_xmtp_sdk_rust_future_free_void,
            liftFunc: { $0 },
            errorHandler: FfiConverterTypeXmtpError_lift
        )
}


open func events(filter: EventFilter)async throws  -> EventReader  {
    return
        try  await uniffiRustCallAsync(
            rustFutureFunc: {
                uniffi_xmtp_sdk_fn_method_client_events(
                        self.uniffiCloneHandle(),FfiConverterTypeEventFilter_lower(filter)
                )
            },
            pollFunc: ffi_xmtp_sdk_rust_future_poll_u64,
            completeFunc: ffi_xmtp_sdk_rust_future_complete_u64,
            freeFunc: ffi_xmtp_sdk_rust_future_free_u64,
            liftFunc: FfiConverterTypeEventReader_lift,
            errorHandler: FfiConverterTypeXmtpError_lift
        )
}

open func storage() -> Storage  {
    return try!  FfiConverterTypeStorage_lift(try! rustCall() {
        uniffiCallStatus in
    uniffi_xmtp_sdk_fn_method_client_storage(
            self.uniffiCloneHandle(),uniffiCallStatus
    )
})
}


}

open class Storage: StorageProtocol, @unchecked Sendable {

open func delete()async throws   {
    return
        try  await uniffiRustCallAsync(
            rustFutureFunc: {
                uniffi_xmtp_sdk_fn_method_storage_delete(
                        self.uniffiCloneHandle()
                )
            },
            pollFunc: ffi_xmtp_sdk_rust_future_poll_void,
            completeFunc: ffi_xmtp_sdk_rust_future_complete_void,
            freeFunc: ffi_xmtp_sdk_rust_future_free_void,
            liftFunc: { $0 },
            errorHandler: FfiConverterTypeXmtpError_lift
        )
}

}

open class MessageReader: MessageReaderProtocol, @unchecked Sendable {

open func next()async throws  -> Message?  {
    return
        try  await uniffiRustCallAsync(
            rustFutureFunc: {
                uniffi_xmtp_sdk_fn_method_messagereader_next(
                        self.uniffiCloneHandle()
                )
            },
            pollFunc: ffi_xmtp_sdk_rust_future_poll_rust_buffer,
            completeFunc: ffi_xmtp_sdk_rust_future_complete_rust_buffer,
            freeFunc: ffi_xmtp_sdk_rust_future_free_rust_buffer,
            liftFunc: FfiConverterOptionTypeMessage.lift,
            errorHandler: FfiConverterTypeXmtpError_lift
        )
}

}
