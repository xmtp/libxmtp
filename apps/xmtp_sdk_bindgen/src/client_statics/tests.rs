use uniffi_meta::{DefaultValueMetadata, LiteralMetadata, ObjectImpl, TraitKind, Type};

use super::*;
use crate::test_metadata::{enum_type, enumeration, field, optional, record, record_type, variant};

fn input(name: &str, ty: Type) -> FnParamMetadata {
    FnParamMetadata::simple(name, ty)
}

fn backend() -> Type {
    Type::Enum {
        module_path: "xmtp_sdk".into(),
        name: BACKEND_SOURCE.into(),
    }
}

fn signer() -> Type {
    Type::Object {
        module_path: "xmtp_sdk".into(),
        name: "Signer".into(),
        imp: ObjectImpl::Trait(TraitKind::Both),
    }
}

fn function(name: &str, inputs: Vec<FnParamMetadata>, doc: Option<&str>) -> Metadata {
    Metadata::Func(FnMetadata {
        module_path: "xmtp_sdk".into(),
        name: name.into(),
        orig_name: None,
        is_async: true,
        inputs,
        return_type: Some(Type::Boolean),
        throws: None,
        checksum: None,
        docstring: doc.map(Into::into),
    })
}

fn marked(name: &str, inputs: Vec<FnParamMetadata>) -> Metadata {
    function(
        name,
        inputs,
        Some("Reads the backend.\n@xmtp-client-static"),
    )
}

fn statics(items: &[Metadata]) -> Result<Vec<(String, Vec<String>)>> {
    let refs = items.iter().collect::<Vec<_>>();
    Ok(client_statics(&refs)?
        .into_iter()
        .map(|item| {
            (
                item.name,
                item.parameters
                    .iter()
                    .map(|parameter| parameter.name.clone())
                    .collect(),
            )
        })
        .collect())
}

// The rule: the name loses `_with_backend`, and the backend moves last.
#[xmtp_common::test(unwrap_try = true)]
fn statics_drop_the_backend_suffix_and_take_the_backend_last() {
    let items = [
        marked(
            "is_address_authorized_with_backend",
            vec![
                input("backend", backend()),
                input("inbox_id", Type::String),
                input("address", Type::String),
            ],
        ),
        marked(
            "fetch_server_configuration",
            vec![input("backend", backend())],
        ),
        marked(
            "verify_signed_with_public_key",
            vec![input("text", Type::String), input("signature", Type::Bytes)],
        ),
        // An unmarked backend function stays a plain function.
        function(
            "latest_inbox_updates_count",
            vec![
                input("inbox_ids", Type::String),
                input("backend", backend()),
            ],
            None,
        ),
    ];
    assert_eq!(
        statics(&items)?,
        [
            (
                "fetchServerConfiguration".to_owned(),
                vec!["backend".to_owned()]
            ),
            (
                "isAddressAuthorized".to_owned(),
                vec!["inbox_id".into(), "address".into(), "backend".into()]
            ),
            (
                "verifySignedWithPublicKey".to_owned(),
                vec!["text".into(), "signature".into()]
            ),
        ]
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn statics_stop_on_a_function_the_rule_cannot_express() {
    let error = |items: Vec<Metadata>| statics(&items).unwrap_err().to_string();
    let mut sync = marked("inbox_states_with_backend", vec![]);
    if let Metadata::Func(function) = &mut sync {
        function.is_async = false;
    }
    assert!(error(vec![sync]).contains("asynchronous"));
    assert!(
        error(vec![function(
            "inbox_states_with_backend",
            vec![],
            Some("@xmtp-client-static @xmtp-internal")
        )])
        .contains("cannot be @xmtp-internal")
    );
    assert!(
        error(vec![marked(
            "copy_with_backend",
            vec![input("from", backend()), input("to", backend())]
        )])
        .contains("at most one BackendSource")
    );
    assert!(error(vec![marked("create_with_backend", vec![])]).contains("host constructor"));
    // TypeScript reads `static constructor` as a constructor declaration.
    assert!(
        error(vec![marked("constructor_with_backend", vec![])])
            .contains("Client.constructor is the class constructor")
    );
    // Swift declares these with their own keyword, not as a method.
    for (name, taken) in [
        ("init", "a Swift initializer"),
        ("deinit", "a Swift deinitializer"),
        ("subscript", "a Swift subscript"),
    ] {
        let message = error(vec![marked(&format!("{name}_with_backend"), vec![])]);
        assert!(
            message.contains(&format!("Client.{name} is {taken}")),
            "{name}: {message}"
        );
    }
    // A TypeScript class is a function, and its own properties are taken.
    for name in ["name", "length", "prototype", "caller", "arguments"] {
        let message = error(vec![marked(&format!("{name}_with_backend"), vec![])]);
        assert!(
            message.contains("a property of every JavaScript function"),
            "{name}: {message}"
        );
    }
    assert!(
        error(vec![
            marked("inbox_states", vec![]),
            marked("inbox_states_with_backend", vec![]),
        ])
        .contains("also Client.inboxStates")
    );
    for name in ["_with_backend", "Inbox_with_backend", "inbóx"] {
        assert!(error(vec![marked(name, vec![])]).contains("lowercase ASCII"));
    }
}

fn defaulted(name: &str, ty: Type) -> FnParamMetadata {
    let mut parameter = input(name, ty);
    parameter.default = Some(DefaultValueMetadata::Literal(LiteralMetadata::None));
    parameter
}

// A caller may leave out a trailing defaulted parameter of the function,
// and of its static too. The moved backend would come after it, and
// TypeScript cannot leave out a parameter before a required one.
#[xmtp_common::test(unwrap_try = true)]
fn statics_keep_a_defaulted_parameter_optional_or_stop() {
    let nonce = || {
        defaulted(
            "nonce",
            Type::Optional {
                inner_type: Box::new(Type::UInt64),
            },
        )
    };
    let error = statics(&[marked(
        "inbox_states_with_backend",
        vec![
            input("backend", backend()),
            input("ids", Type::String),
            nonce(),
        ],
    )])
    .unwrap_err()
    .to_string();
    assert!(
            error.starts_with("inbox_states_with_backend.nonce: the static takes the BackendSource after this defaulted parameter"),
            "{error}"
        );
    let mut defaulted_backend = input("backend", backend());
    defaulted_backend.default = Some(DefaultValueMetadata::Default);
    let error = statics(&[marked(
        "inbox_states_with_backend",
        vec![defaulted_backend, input("ids", Type::String), nonce()],
    )])
    .unwrap_err()
    .to_string();
    assert!(
            error.starts_with(
                "inbox_states_with_backend.backend: a client static's BackendSource cannot have a default"
            ),
            "{error}"
        );
    // Without a backend the order stays, and so does the default.
    let items = [marked(
        "inbox_states",
        vec![input("ids", Type::String), nonce()],
    )];
    let refs = items.iter().collect::<Vec<_>>();
    let code = crate::public_projection::client_members_for_test(&refs)?;
    assert!(
            code.contains("static inboxStates(ids: string, nonce?: bigint): Promise<boolean> {\nreturn inboxStates(ids, nonce);\n}"),
            "{code}"
        );
    // A reserved word keeps its default under its TypeScript name.
    let items = [marked(
        "inbox_states",
        vec![
            input("ids", Type::String),
            defaulted(
                "default",
                Type::Optional {
                    inner_type: Box::new(Type::UInt64),
                },
            ),
        ],
    )];
    let refs = items.iter().collect::<Vec<_>>();
    let code = crate::public_projection::client_members_for_test(&refs)?;
    assert!(
            code.contains("static inboxStates(ids: string, default_?: bigint): Promise<boolean> {\nreturn inboxStates(ids, default_);\n}"),
            "{code}"
        );
}

// The public Client inherits the statics of the generated ClientMembers.
#[xmtp_common::test(unwrap_try = true)]
fn typescript_statics_call_the_public_function_with_the_backend_last() {
    let items = [
        marked(
            "is_address_authorized_with_backend",
            vec![
                input("backend", backend()),
                input("inbox_id", Type::String),
                input("address", Type::String),
            ],
        ),
        function(
            "latest_inbox_updates_count",
            vec![
                input("inbox_ids", Type::String),
                input("backend", backend()),
            ],
            None,
        ),
    ];
    let refs = items.iter().collect::<Vec<_>>();
    let code = crate::public_projection::client_members_for_test(&refs)?;
    assert!(code.contains(
            "static isAddressAuthorized(inboxId: string, address: string, backend: BackendSource): Promise<boolean> {\nreturn isAddressAuthorizedWithBackend(backend, inboxId, address);\n}"
        ));
    assert_eq!(code.matches("static ").count(), 1);
    assert!(code.ends_with("}\n}\n"));
}

const KOTLIN: &str = "\
    @Throws(XmtpException::class)
     suspend fun `isAddressAuthorizedWithBackend`(`backend`: BackendSource, `inboxId`: InboxId, `address`: kotlin.String) : kotlin.Boolean {
    }
     suspend fun `revokeInstallationsWithBackend`(`backend`: BackendSource, `signer`: Signer, `ids`: Map<kotlin.String, List<InstallationId>>) {
    }
     suspend fun `create`(`signer`: Signer) : Client {
";

#[xmtp_common::test(unwrap_try = true)]
fn kotlin_statics_extend_the_companion_and_wrap_foreign_values() {
    let items = [
        marked(
            "is_address_authorized_with_backend",
            vec![
                input("backend", backend()),
                input("inbox_id", Type::String),
                input("address", Type::String),
            ],
        ),
        marked(
            "revoke_installations_with_backend",
            vec![
                input("backend", backend()),
                input("signer", signer()),
                input("ids", Type::String),
            ],
        ),
    ];
    let refs = items.iter().collect::<Vec<_>>();
    let code = kotlin(&client_statics(&refs)?, KOTLIN, &refs)?;
    assert_eq!(
            code,
            "\
suspend fun SDKClient.Companion.`isAddressAuthorized`(`inboxId`: InboxId, `address`: kotlin.String, `backend`: BackendSource): kotlin.Boolean =
    uniffi.xmtp_sdk.`isAddressAuthorizedWithBackend`(SDKForeign.backend(`backend`), `inboxId`, `address`)

suspend fun SDKClient.Companion.`revokeInstallations`(`signer`: Signer, `ids`: Map<kotlin.String, List<InstallationId>>, `backend`: BackendSource) =
    uniffi.xmtp_sdk.`revokeInstallationsWithBackend`(SDKForeign.backend(`backend`), SDKForeign.signer(`signer`), `ids`)

"
        );
}

#[xmtp_common::test(unwrap_try = true)]
fn kotlin_statics_stop_on_a_binding_they_cannot_read() {
    let render = |items: Vec<Metadata>, binding: &str| {
        let refs = items.iter().collect::<Vec<_>>();
        kotlin(&client_statics(&refs).unwrap(), binding, &refs)
            .unwrap_err()
            .to_string()
    };
    let address = || {
        marked(
            "is_address_authorized_with_backend",
            vec![
                input("backend", backend()),
                input("inbox_id", Type::String),
                input("address", Type::String),
            ],
        )
    };
    assert!(
        render(vec![address()], "")
            .contains("has no `suspend fun `isAddressAuthorizedWithBackend`(`")
    );
    assert!(render(vec![address()], &format!("{KOTLIN}{KOTLIN}")).contains("more than once"));
    // A type never lands on another parameter.
    assert!(
        render(
            vec![marked(
                "is_address_authorized_with_backend",
                vec![
                    input("backend", backend()),
                    input("address", Type::String),
                    input("inbox_id", Type::String),
                ],
            )],
            KOTLIN
        )
        .contains(
            "declares (backend, inboxId, address), but the metadata has (backend, address, inboxId)"
        )
    );
    // A foreign trait without a wrapper, or inside a container, stops.
    let listener = Type::CallbackInterface {
        module_path: "xmtp_sdk".into(),
        name: "EventListener".into(),
    };
    let binding = "     suspend fun `watchWithBackend`(`backend`: BackendSource, `listener`: EventListener) {\n";
    assert!(
        render(
            vec![marked(
                "watch_with_backend",
                vec![input("backend", backend()), input("listener", listener)]
            )],
            binding
        )
        .contains("EventListener has no wrapper")
    );
    let binding = "     suspend fun `watchWithBackend`(`backend`: BackendSource, `signers`: List<Signer>) {\n";
    assert!(
        render(
            vec![marked(
                "watch_with_backend",
                vec![
                    input("backend", backend()),
                    input(
                        "signers",
                        Type::Sequence {
                            inner_type: Box::new(signer())
                        }
                    )
                ]
            )],
            binding
        )
        .contains("signers: it holds a Signer that no SDKForeign wrapper reaches")
    );
}

// A record or enum that holds a foreign value has no wrapper: the value
// would reach Rust unwrapped. BackendOptions holds a CredentialSource,
// and ClientOptions a BackendSource that holds BackendOptions.
#[xmtp_common::test(unwrap_try = true)]
fn kotlin_statics_stop_on_a_foreign_value_inside_a_record_or_enum() {
    let credentials = Type::CallbackInterface {
        module_path: "xmtp_sdk".into(),
        name: "CredentialSource".into(),
    };
    let types = || {
        vec![
            record(
                "BackendOptions",
                vec![
                    field("url", Type::String, None),
                    field("credentials", optional(credentials.clone()), None),
                ],
            ),
            enumeration(
                BACKEND_SOURCE,
                vec![variant(
                    "Options",
                    None,
                    vec![field("options", record_type("BackendOptions"), None)],
                )],
            ),
            record(
                "ClientOptions",
                vec![field("backend", optional(backend()), None)],
            ),
            record(
                "PublicIdentity",
                vec![field("identifier", Type::String, None)],
            ),
            enumeration(
                "SignIn",
                vec![variant(
                    "Remote",
                    None,
                    vec![field("credentials", credentials.clone(), None)],
                )],
            ),
        ]
    };
    let render = |parameter: &str, ty: Type| {
        let mut items = types();
        items.push(marked("probe_with_backend", vec![input(parameter, ty)]));
        let refs = items.iter().collect::<Vec<_>>();
        let binding = format!("     suspend fun `probeWithBackend`(`{parameter}`: T) {{\n");
        kotlin(&client_statics(&refs)?, &binding, &refs)
    };
    let error = render("options", record_type("BackendOptions"))
        .unwrap_err()
        .to_string();
    assert!(
            error.contains("probe_with_backend.options: it holds a CredentialSource that no SDKForeign wrapper reaches"),
            "{error}"
        );
    let error = render("options", record_type("ClientOptions"))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("options: it holds a BackendSource that no SDKForeign wrapper reaches"),
        "{error}"
    );
    let error = render("choice", enum_type("SignIn"))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("choice: it holds a CredentialSource that no SDKForeign wrapper reaches"),
        "{error}"
    );
    // The backend's own wrapper reaches the credentials inside it, and a
    // record without a foreign value passes as it is.
    assert!(render("backend", backend())?.contains("(SDKForeign.backend(`backend`))"));
    assert!(
        render("identity", record_type("PublicIdentity"))?
            .contains("`probeWithBackend`(`identity`)")
    );
}

// A Swift keyword stays quoted wherever the call names it: the module
// function, and an argument used as an expression.
#[xmtp_common::test(unwrap_try = true)]
fn swift_statics_quote_keyword_names() {
    let binding = "\
public func `switch`(backend: BackendSource, default: Int32, `var`: Bool)async throws   {
";
    let items = [marked(
        "switch",
        vec![
            input("backend", backend()),
            input("default", Type::Int32),
            input("var", Type::Boolean),
        ],
    )];
    let refs = items.iter().collect::<Vec<_>>();
    let code = swift(&client_statics(&refs)?, binding)?;
    assert!(
        code.contains(
            "try await XmtpSdk.`switch`(backend: backend, default: `default`, `var`: `var`)"
        ),
        "{code}"
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn swift_statics_label_every_parameter_and_call_the_module_function() {
    let binding = "\
public func fetchServerConfiguration(backend: BackendSource)async throws  -> ServerConfiguration  {
public func canMessageWithBackend(backend: BackendSource, identities: [PublicIdentity], options: [String: Bool]? = nil)async throws  -> [String: Bool]  {
public func revokeInstallationsWithBackend(backend: BackendSource, signer: Signer)async throws   {
";
    let items = [
        marked(
            "fetch_server_configuration",
            vec![input("backend", backend())],
        ),
        marked(
            "can_message_with_backend",
            vec![
                input("backend", backend()),
                input("identities", Type::String),
                input("options", Type::String),
            ],
        ),
        marked(
            "revoke_installations_with_backend",
            vec![input("backend", backend()), input("signer", signer())],
        ),
    ];
    let refs = items.iter().collect::<Vec<_>>();
    assert_eq!(
            swift(&client_statics(&refs)?, binding)?,
            "    static func `canMessage`(identities: [PublicIdentity], options: [String: Bool]? = nil, backend: BackendSource) async throws -> [String: Bool] {
        try await XmtpSdk.canMessageWithBackend(backend: backend, identities: identities, options: options)
    }

    static func `fetchServerConfiguration`(backend: BackendSource) async throws -> ServerConfiguration {
        try await XmtpSdk.fetchServerConfiguration(backend: backend)
    }

    static func `revokeInstallations`(signer: Signer, backend: BackendSource) async throws {
        try await XmtpSdk.revokeInstallationsWithBackend(backend: backend, signer: signer)
    }

"
        );
    let refs = items[..1].iter().collect::<Vec<_>>();
    assert!(
            swift(&client_statics(&refs)?, "public func fetchServerConfiguration(backend: BackendSource) -> ServerConfiguration {\n")
                .unwrap_err()
                .to_string()
                .contains("not async")
        );
}

#[xmtp_common::test(unwrap_try = true)]
fn parameter_lists_keep_nested_commas_and_arrows() {
    let (parameters, rest) = parameter_list(
        "a: Map<K, V>, b: (Int, Int) -> Void, c: [String: [Int]], d: Map<(Int) -> Void, Int>) async {",
        "f",
    )?;
    assert_eq!(
        parameters,
        [
            "a: Map<K, V>",
            "b: (Int, Int) -> Void",
            "c: [String: [Int]]",
            "d: Map<(Int) -> Void, Int>"
        ]
    );
    assert_eq!(rest, " async {");
    assert_eq!(parameter_list(") {", "f")?, (vec![], " {"));
    assert!(parameter_list("a: Int", "f").is_err());
}
