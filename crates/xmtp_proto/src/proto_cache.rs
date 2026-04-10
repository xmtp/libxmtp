//! Cache for Proto definitions

use prost::Message;
use prost_types::{FileDescriptorSet, MethodDescriptorProto, ServiceDescriptorProto};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::LazyLock;

type Cache =
    HashMap<(Cow<'static, str>, Cow<'static, str>, Cow<'static, str>), Cow<'static, str>>;

// lookup a method path&query based on package, service name, and type name
pub static METHOD_LOOKUP: LazyLock<Cache> = LazyLock::new(|| {
    use Cow::*;
    let pnq = |package: &str,
               service: &ServiceDescriptorProto,
               method: &MethodDescriptorProto|
     -> String {
        String::new() + "/" + package + "." + service.name() + "/" + method.name()
    };
    let mut map = HashMap::new();
    let descriptors: FileDescriptorSet =
        Message::decode(crate::FILE_DESCRIPTOR_SET).expect("static decode must always succeed");
    let mut dcs = descriptors.file.iter();
    loop {
        let Some(fd) = dcs.next() else {
            break map;
        };
        let Some(ref package) = fd.package else {
            continue;
        };
        for service in fd.service.iter() {
            let svc_name = service.name().to_string();
            for method in service.method.iter() {
                if let Some(output_t) = method.output_type().split('.').next_back() {
                    map.insert(
                        (
                            Owned(package.clone()),
                            Owned(svc_name.clone()),
                            Owned(output_t.to_string()),
                        ),
                        Owned(pnq(package, service, method)),
                    );
                };

                let Some(input_t) = method.input_type().split('.').next_back() else {
                    continue;
                };
                map.insert(
                    (
                        Owned(package.clone()),
                        Owned(svc_name.clone()),
                        Owned(input_t.to_string()),
                    ),
                    Owned(pnq(package, service, method)),
                );
            }
        }
    }
});

pub fn path_and_query<Type: prost::Name>(service: &str) -> Cow<'static, str> {
    METHOD_LOOKUP
        .get(&(
            Cow::Borrowed(Type::PACKAGE),
            Cow::Borrowed(service),
            Cow::Borrowed(Type::NAME),
        ))
        .cloned()
        .unwrap_or(Cow::Owned(String::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test]
    fn method_lookup() {
        println!("{:#?}", METHOD_LOOKUP.iter().collect::<Vec<_>>());
    }
}
