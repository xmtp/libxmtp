//! Separate binary because this test installs the process-global logger and subscriber.
#![cfg(not(target_arch = "wasm32"))]

use std::{
    io::Write,
    sync::{Arc, Mutex},
};

use openmls::group::{GroupId, MlsGroup};
use tracing_subscriber::{EnvFilter, Registry, fmt, prelude::*};
use xmtp_mls::{context::XmtpSharedContext, tester};
use xmtp_mls_common::app_data::{
    component_id::ComponentId, component_source::read_from_app_data_dict,
};

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// verifies: GMOD-042
#[xmtp_common::test(unwrap_try = true, disable_logging = true)]
async fn host_subscriber_never_receives_the_group_root() {
    let capture = Capture::default();
    let writer = capture.clone();
    let subscriber = Registry::default().with(
        fmt::layer()
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .with_filter(EnvFilter::new("trace")),
    );
    tracing::subscriber::set_global_default(subscriber)?;
    tracing_log::LogTracer::init()?;

    tester!(alix);
    let conversation = alix.create_group(None, None)?;
    let group = MlsGroup::load(
        alix.context.mls_storage(),
        &GroupId::from_slice(&conversation.group_id),
    )?
    .expect("created group is stored");
    let root = read_from_app_data_dict(ComponentId::GROUP_ROOT_KEY, &group)
        .expect("created group has a root");
    assert_eq!(root.len(), 32);

    let output = String::from_utf8(capture.0.lock().unwrap().clone())?;
    assert!(
        output.contains("openmls::schedule"),
        "host subscriber received no OpenMLS schedule record"
    );
    for (name, rendered) in [
        ("decimal list", format!("{root:?}")),
        ("hexadecimal list", format!("{root:x?}")),
        ("KDF Debug", format!("0x{}", hex::encode(root))),
    ] {
        // The root is embedded in the context, not formatted as a standalone value.
        let bytes = rendered.trim_start_matches("0x").trim_matches(['[', ']']);
        assert!(
            !output.contains(bytes),
            "host subscriber received the group root as {name}"
        );
    }
}
