//! Cache for Proto definitions

use once_cell::sync::Lazy;
use parking_lot::RwLock;
use prost::Message;
use prost_types::{FileDescriptorSet, MethodDescriptorProto, ServiceDescriptorProto};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::LazyLock;

type Cache = HashMap<
    (Cow<'static, str>, Cow<'static, str>),
    (ServiceDescriptorProto, MethodDescriptorProto),
>;

pub static PROTO_CACHE: Lazy<RwLock<Cache>> = Lazy::new(|| RwLock::new(HashMap::new()));

static DESCRIPTOR_SET: LazyLock<FileDescriptorSet> = LazyLock::new(|| {
    Message::decode(xmtp_proto::FILE_DESCRIPTOR_SET).expect("static decode must always succeed")
});

// maybe by using crabtime? https://docs.rs/crabtime/latest/crabtime/
// just need to collect the file descriptor set and created a static lookup table
pub fn path_and_query<Type: prost::Name>() -> Cow<'static, str> {
    let pnq = |service: &ServiceDescriptorProto, method: &MethodDescriptorProto| -> String {
        String::new() + "/" + Type::PACKAGE + "." + service.name() + "/" + method.name()
    };
    if let Some((service, method)) = crate::PROTO_CACHE
        .read()
        .get(&(Cow::Borrowed(Type::PACKAGE), Cow::Borrowed(Type::NAME)))
    {
        return Cow::Owned(pnq(service, method));
    }

    // we generate fds for all our compiled protos
    let Some((service, method)) = DESCRIPTOR_SET.file.iter().find_map(|f| {
        if f.package == Some(Type::PACKAGE.to_string()) {
            f.service.iter().find_map(|s| {
                let method = s
                    .method
                    .iter()
                    .find(|m| m.input_type().ends_with(Type::NAME));
                method.map(|m| (s.clone(), m.clone()))
            })
        } else {
            None
        }
    }) else {
        panic!("static proto `Type` must be described in static file")
    };

    let mut map = crate::PROTO_CACHE.write();
    let path_and_query = Cow::Owned(pnq(&service, &method));
    map.insert(
        (Cow::Borrowed(Type::PACKAGE), Cow::Borrowed(Type::NAME)),
        (service, method.clone()),
    );
    path_and_query
}
