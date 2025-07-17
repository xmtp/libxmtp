//! Cache for Proto definitions

use once_cell::sync::Lazy;
use parking_lot::RwLock;
use prost::Message;
use prost_types::{FileDescriptorSet, MethodDescriptorProto, ServiceDescriptorProto};
use std::borrow::Cow;
use std::collections::HashMap;

type Cache = HashMap<Cow<'static, str>, (ServiceDescriptorProto, MethodDescriptorProto)>;

pub static PROTO_CACHE: Lazy<RwLock<Cache>> = Lazy::new(|| RwLock::new(HashMap::new()));

// TODO: Create proc macro to get FILE_DESCRIPTOR from rust path of `Type` to remove
// file_descriptor arg
// maybe by using eval macro? https://docs.rs/eval-macro/latest/eval_macro/
// just need to collect the file descriptor set and created a static lookup table
pub fn path_and_query<Type: prost::Name>(file_descriptor: &'static [u8]) -> Cow<'static, str> {
    let pnq = |service: &ServiceDescriptorProto, method: &MethodDescriptorProto| -> String {
        String::new() + "/" + Type::PACKAGE + "." + service.name() + "/" + method.name()
    };
    if let Some((service, method)) = crate::PROTO_CACHE
        .read()
        .get((Type::PACKAGE.to_owned() + "." + Type::NAME).as_str())
    {
        return Cow::Owned(pnq(service, method));
    }

    let fds: FileDescriptorSet = Message::decode(file_descriptor).unwrap();
    // protoc-gen-prost explicitly generates file descriptors with one per module
    let Some((service, method)) = fds.file.into_iter().find_map(|f| {
        f.service.into_iter().find_map(|s| {
            let method = s
                .method
                .iter()
                .find(|m| m.input_type().ends_with(Type::NAME));
            method.map(|m| (s.clone(), m.clone()))
        })
    }) else {
        panic!("static proto `Type` must be described in static file")
    };

    let mut map = crate::PROTO_CACHE.write();
    let path_and_query = Cow::Owned(pnq(&service, &method));
    map.insert(Cow::Borrowed(Type::NAME), (service, method.clone()));
    path_and_query
}
