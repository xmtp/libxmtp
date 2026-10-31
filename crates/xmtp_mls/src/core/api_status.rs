/// The status the backend returned for a publish. A failed configuration
/// check before dispatch sent nothing, so its status is not the backend's.
pub(crate) fn backend_status(error: &xmtp_api::ApiError) -> Option<tonic::Code> {
    if xmtp_api::preflight::failure(error).is_some() {
        return None;
    }
    xmtp_proto::api::grpc_status(error).map(|status| status.code())
}

/// Only `INVALID_ARGUMENT` proves that an atomic publish stored nothing.
/// Every other failure, including `OUT_OF_RANGE`, can follow a commit.
pub(crate) fn definite_refusal(error: &xmtp_api::ApiError) -> bool {
    backend_status(error) == Some(tonic::Code::InvalidArgument)
}
