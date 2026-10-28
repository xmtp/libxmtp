//! Every RPC the backend serves has a charge decision.
use super::super::charges::{CHARGES, Charges};
use super::*;
use prost::Message;
use std::collections::BTreeSet;

/// The path of every method of every service the backend serves, from the
/// descriptors its services are generated from.
fn served_methods() -> TestResult<BTreeSet<String>> {
    let served: BTreeSet<&str> = crate::server::SERVICES
        .into_iter()
        .chain(["grpc.health.v1.Health"])
        .collect();
    let mut found = BTreeSet::new();
    let mut methods = BTreeSet::new();
    for set in [
        xmtp_proto::FILE_DESCRIPTOR_SET,
        tonic_health::pb::FILE_DESCRIPTOR_SET,
    ] {
        for file in prost_types::FileDescriptorSet::decode(set)?.file {
            for service in &file.service {
                let name = format!("{}.{}", file.package(), service.name());
                if !served.contains(name.as_str()) {
                    continue;
                }
                found.insert(name.clone());
                for method in &service.method {
                    methods.insert(format!("/{name}/{}", method.name()));
                }
            }
        }
    }
    // A served service missing from the descriptors would pass vacuously.
    assert_eq!(
        found,
        served.iter().map(|name| name.to_string()).collect(),
        "every served service has a descriptor"
    );
    Ok(methods)
}

// verifies: RATE-011
#[xmtp_common::test(unwrap_try = true)]
fn every_served_method_has_one_charge_decision() {
    let methods = served_methods()?;
    let rows: Vec<&str> = CHARGES.iter().map(|(path, _)| *path).collect();
    for method in &methods {
        assert_eq!(
            rows.iter().filter(|row| **row == method).count(),
            1,
            "{method} needs one row in charges.rs"
        );
    }
    for row in &rows {
        assert!(methods.contains(*row), "{row} is not a served method");
    }
}

// verifies: RATE-011
#[xmtp_common::test(unwrap_try = true)]
fn only_health_and_configuration_are_exempt_and_every_charge_starts_with_a_request() {
    for (path, charges) in CHARGES {
        match charges {
            Charges::Exempt => assert!(
                path.starts_with("/grpc.health.v1.")
                    || path.starts_with("/xmtp.backend.v1.ConfigurationService/"),
                "{path} must be charged"
            ),
            Charges::Charged(categories) => {
                assert_eq!(categories.first(), Some(&Category::Request), "{path}");
                assert_eq!(
                    categories
                        .iter()
                        .collect::<std::collections::HashSet<_>>()
                        .len(),
                    categories.len(),
                    "{path} lists a category twice"
                );
            }
        }
    }
}
