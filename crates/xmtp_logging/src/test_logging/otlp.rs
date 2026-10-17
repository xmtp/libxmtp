//! Local OTLP receiver shared by telemetry and binding tests.
use opentelemetry_proto::tonic::{
    collector::{
        logs::v1::{
            ExportLogsServiceRequest, ExportLogsServiceResponse,
            logs_service_server::{LogsService, LogsServiceServer},
        },
        trace::v1::{
            ExportTraceServiceRequest, ExportTraceServiceResponse,
            trace_service_server::{TraceService, TraceServiceServer},
        },
    },
    logs::v1::ResourceLogs,
    trace::v1::ResourceSpans,
};
use parking_lot::Mutex;
use std::sync::Arc;
use tonic::{Request, Response, Status};

/// Read a string attribute from a received OTLP resource or span.
pub fn string_attribute<'a>(
    attributes: &'a [opentelemetry_proto::tonic::common::v1::KeyValue],
    key: &str,
) -> Option<&'a str> {
    use opentelemetry_proto::tonic::common::v1::any_value::Value;
    let value = attributes.iter().find(|attribute| attribute.key == key)?;
    match value.value.as_ref()?.value.as_ref()? {
        Value::StringValue(value) => Some(value),
        _ => None,
    }
}

#[derive(Clone, Default)]
struct Receiver {
    spans: Arc<Mutex<Vec<ResourceSpans>>>,
    trace_headers: Arc<Mutex<Vec<http::HeaderMap>>>,
    logs: Arc<Mutex<Vec<ResourceLogs>>>,
}

#[tonic::async_trait]
impl TraceService for Receiver {
    async fn export(
        &self,
        request: Request<ExportTraceServiceRequest>,
    ) -> Result<Response<ExportTraceServiceResponse>, Status> {
        self.trace_headers
            .lock()
            .push(request.metadata().clone().into_headers());
        self.spans
            .lock()
            .extend(request.into_inner().resource_spans);
        Ok(Response::new(ExportTraceServiceResponse::default()))
    }
}

#[tonic::async_trait]
impl LogsService for Receiver {
    async fn export(
        &self,
        request: Request<ExportLogsServiceRequest>,
    ) -> Result<Response<ExportLogsServiceResponse>, Status> {
        self.logs.lock().extend(request.into_inner().resource_logs);
        Ok(Response::new(ExportLogsServiceResponse::default()))
    }
}

/// A real local OTLP gRPC server. Drop stops its task; no external collector is needed.
pub struct OtlpCollector {
    endpoint: String,
    receiver: Receiver,
    task: tokio::task::JoinHandle<Result<(), tonic::transport::Error>>,
}

impl OtlpCollector {
    /// Bind an ephemeral loopback port and start accepting OTLP requests.
    pub async fn start() -> std::io::Result<Self> {
        const LOOPBACK_EPHEMERAL: &str = "127.0.0.1:0";
        let listener = tokio::net::TcpListener::bind(LOOPBACK_EPHEMERAL).await?;
        let endpoint = format!("http://{}", listener.local_addr()?);
        let receiver = Receiver::default();
        let server = tonic::transport::Server::builder()
            .add_service(TraceServiceServer::new(receiver.clone()))
            .add_service(LogsServiceServer::new(receiver.clone()));
        let task = tokio::spawn(
            server.serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener)),
        );
        Ok(Self {
            endpoint,
            receiver,
            task,
        })
    }

    /// Endpoint for TelemetryConfig.
    pub fn endpoint(&self) -> String {
        self.endpoint.clone()
    }

    /// Remove and return all received span batches.
    pub fn take_spans(&self) -> Vec<ResourceSpans> {
        std::mem::take(&mut *self.receiver.spans.lock())
    }

    /// Remove and return metadata from received trace RPCs.
    pub fn take_trace_headers(&self) -> Vec<http::HeaderMap> {
        std::mem::take(&mut *self.receiver.trace_headers.lock())
    }

    /// Remove and return all received log batches.
    pub fn take_logs(&self) -> Vec<ResourceLogs> {
        std::mem::take(&mut *self.receiver.logs.lock())
    }
}

impl Drop for OtlpCollector {
    fn drop(&mut self) {
        self.task.abort();
    }
}
