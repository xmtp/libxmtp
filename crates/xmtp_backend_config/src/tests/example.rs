//! The CONF spec's configuration field table, its compiled defaults, and its
//! example file, read from the spec itself. The tests here and in the parent
//! module keep the spec and the configuration code equal.

use super::*;
use std::collections::BTreeSet;

const SPEC: &str = include_str!("../../../../docs/specs/CONF-backend-configuration.md");
const FIELD_TABLE_HEADER: &str = "| Key | Type | Presence | Default | Checks | Notes |";
const DEFAULTS_TABLE_HEADER: &str = "| Field | Default value |";
const CHILD: &str = "XMTP_CONFIG_EXAMPLE_CHILD";

/// One row of the configuration field table, with code-span backticks kept
/// in every cell except the key.
pub(super) struct FieldRow {
    pub(super) key: String,
    pub(super) kind: String,
    pub(super) default: String,
    pub(super) checks: String,
}

pub(super) fn field_table() -> Vec<FieldRow> {
    let mut lines = SPEC.lines().skip_while(|line| *line != FIELD_TABLE_HEADER);
    assert!(
        lines.next().is_some(),
        "CONF has no configuration field table"
    );
    lines.next();
    let rows: Vec<FieldRow> = lines
        .take_while(|line| line.starts_with('|'))
        .map(|line| {
            let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
            assert_eq!(cells.len(), 6, "{line}");
            FieldRow {
                key: code(cells[0]).to_owned(),
                kind: cells[1].to_owned(),
                default: cells[3].to_owned(),
                checks: cells[4].to_owned(),
            }
        })
        .collect();
    assert!(!rows.is_empty(), "the configuration field table is empty");
    rows
}

/// The compiled defaults of CONF section 3, keyed by the field name in the
/// first code span of each row.
pub(super) fn compiled_defaults() -> BTreeMap<String, String> {
    let mut lines = SPEC
        .lines()
        .skip_while(|line| !line.starts_with("## 3. "))
        .skip_while(|line| *line != DEFAULTS_TABLE_HEADER);
    assert!(
        lines.next().is_some(),
        "CONF section 3 has no defaults table"
    );
    lines.next();
    lines
        .take_while(|line| line.starts_with('|'))
        .map(|line| {
            let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
            assert_eq!(cells.len(), 2, "{line}");
            let field = cells[0].split('`').nth(1).unwrap_or(cells[0]);
            (field.to_owned(), cells[1].to_owned())
        })
        .collect()
}

/// The text inside one code span, or the text unchanged.
pub(super) fn code(cell: &str) -> &str {
    cell.strip_prefix('`')
        .and_then(|cell| cell.strip_suffix('`'))
        .unwrap_or(cell)
}

/// A key with every operator-chosen name, such as `<chain id>`, written as `<*>`.
pub(super) fn key_pattern(key: &str) -> String {
    let mut pattern = String::new();
    let mut rest = key;
    while let Some(open) = rest.find('<') {
        let close = open + rest[open..].find('>').expect("an unclosed <");
        pattern.push_str(&rest[..open]);
        pattern.push_str("<*>");
        rest = &rest[close + 1..];
    }
    pattern.push_str(rest);
    pattern
}

/// Every property path of the generated schema, in the form of the field
/// table's Key column, with `<*>` for an operator-chosen name.
pub(super) fn schema_paths() -> BTreeSet<String> {
    let schema = serde_json::to_value(Config::schema()).expect("schema serializes");
    let mut paths = BTreeSet::new();
    collect_schema_paths(&schema, &schema["$defs"], "", &mut paths);
    paths
}

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}.{name}")
    }
}

fn collect_schema_paths(
    node: &serde_json::Value,
    definitions: &serde_json::Value,
    prefix: &str,
    paths: &mut BTreeSet<String>,
) {
    if let Some(reference) = node.get("$ref").and_then(serde_json::Value::as_str) {
        let name = reference
            .rsplit('/')
            .next()
            .expect("a $ref names a definition");
        collect_schema_paths(&definitions[name], definitions, prefix, paths);
    }
    for keyword in ["anyOf", "oneOf", "allOf"] {
        for variant in node
            .get(keyword)
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            collect_schema_paths(variant, definitions, prefix, paths);
        }
    }
    if let Some(properties) = node
        .get("properties")
        .and_then(serde_json::Value::as_object)
    {
        for (name, property) in properties {
            let path = join(prefix, name);
            paths.insert(path.clone());
            collect_schema_paths(property, definitions, &path, paths);
        }
    }
    if let Some(values) = node
        .get("additionalProperties")
        .filter(|values| values.is_object())
    {
        let path = join(prefix, "<*>");
        paths.insert(path.clone());
        collect_schema_paths(values, definitions, &path, paths);
    }
    if let Some(items) = node.get("items").filter(|items| items.is_object()) {
        collect_schema_paths(items, definitions, &format!("{prefix}[]"), paths);
    }
}

/// The fenced `toml` block of CONF section 1.3.
fn example() -> &'static str {
    let section = &SPEC[SPEC
        .find("### 1.3 Example configuration")
        .expect("CONF has section 1.3")..];
    let open = section
        .find("```toml\n")
        .expect("section 1.3 has a toml block")
        + 8;
    let body = &section[open..];
    &body[..body.find("\n```").expect("the toml block is closed") + 1]
}

/// The lines of `lines`, without the ones `drop` selects, with `insert` placed
/// before line `at`.
fn rewrite(
    lines: &[&str],
    drop: impl Fn(usize, &str) -> bool,
    at: usize,
    insert: &[&str],
) -> String {
    let mut document = String::new();
    for (index, line) in lines.iter().enumerate() {
        if index == at {
            for inserted in insert {
                document.push_str(inserted);
                document.push('\n');
            }
        }
        if !drop(index, line) {
            document.push_str(line);
            document.push('\n');
        }
    }
    document
}

/// An example file as written, then with each commented alternative applied
/// in place of the key it replaces: `[[auth.keys]]` for `auth.jwks_url`, and
/// each other credential `kind` with the keys it takes.
fn example_documents(example: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = example.lines().collect();
    let mut documents = vec![("the example".to_owned(), example.to_owned())];

    let keys_start = lines
        .iter()
        .position(|line| *line == "# [[auth.keys]]")
        .expect("the example shows [[auth.keys]] in a comment");
    let keys: Vec<&str> = lines[keys_start..]
        .iter()
        .take_while(|line| line.starts_with("# "))
        .map(|line| &line[2..])
        .collect();
    let keys_end = keys_start + keys.len();
    documents.push((
        "[[auth.keys]] in place of auth.jwks_url".to_owned(),
        rewrite(
            &lines,
            |index, line| {
                line.starts_with("jwks_url = ") || (keys_start..keys_end).contains(&index)
            },
            keys_start,
            &keys,
        ),
    ));

    let kind_line = lines
        .iter()
        .position(|line| line.starts_with("kind = "))
        .expect("the example sets a credential kind");
    let mut index = kind_line + 1;
    while index < lines.len() {
        if !lines[index].starts_with("# kind = ") {
            index += 1;
            continue;
        }
        let mut alternative = vec![&lines[index][2..]];
        index += 1;
        while index < lines.len()
            && lines[index].starts_with("# ")
            && !lines[index].starts_with("# kind = ")
        {
            alternative.push(&lines[index][2..]);
            index += 1;
        }
        documents.push((
            format!("credentials {}", alternative[0]),
            rewrite(&lines, |at, _| at == kind_line, kind_line, &alternative),
        ));
    }
    documents
}

/// The credential kinds the schema defines.
fn schema_credential_kinds() -> BTreeSet<String> {
    let schema = serde_json::to_value(Config::schema()).expect("schema serializes");
    schema["$defs"]["CredentialsConfig"]["oneOf"]
        .as_array()
        .expect("credential kinds are a oneOf")
        .iter()
        .map(|variant| {
            variant["properties"]["kind"]["const"]
                .as_str()
                .expect("each credential variant names its kind")
                .to_owned()
        })
        .collect()
}

/// The paths a document sets, in the form of the field table's Key column.
fn document_paths(
    table: &toml::Table,
    prefix: &str,
    schema: &BTreeSet<String>,
    paths: &mut BTreeSet<String>,
) {
    for (name, value) in table {
        let named = join(prefix, name);
        let path = if schema.contains(&named) {
            named.clone()
        } else {
            join(prefix, "<*>")
        };
        assert!(
            schema.contains(&path),
            "the example sets {named}, which the schema does not define"
        );
        paths.insert(path.clone());
        match value {
            toml::Value::Table(table) => document_paths(table, &path, schema, paths),
            toml::Value::Array(items) => {
                for item in items {
                    if let toml::Value::Table(table) = item {
                        document_paths(table, &format!("{path}[]"), schema, paths);
                    }
                }
            }
            _ => {}
        }
    }
}

/// A value for each environment variable the example references.
fn variable(name: &str) -> String {
    match name {
        "XMTP_OTLP_ENDPOINT" => "https://otel.example.org:4317".into(),
        "XMTP_DATABASE_URL" => "postgres://xmtp:password@db.example.org:5432/xmtp".into(),
        "XMTP_REPLICA_URL" => "postgres://xmtp:password@replica.example.org:5432/xmtp".into(),
        "XMTP_CHAIN_1_RPC_URL" => "https://rpc.example.org/1".into(),
        "XMTP_CHAIN_8453_RPC_URL" => "https://rpc.example.org/8453".into(),
        // A real key, so the documents also start a backend unchanged.
        "XMTP_JWT_PUBLIC_KEY" => ES256_PUBLIC_KEY.into(),
        "XMTP_API_KEY_NOTIFICATION_BOT" | "XMTP_API_KEY_SUPPORT_BRIDGE" => api_key_value(),
        "XMTP_APNS_KEY" => ES256_PRIVATE_KEY.into(),
        "XMTP_FCM_SERVICE_ACCOUNT" => serde_json::json!({
            "type": "service_account",
            "project_id": "example-project",
            "client_email": "push@example-project.iam.gserviceaccount.com",
            "private_key": "-----BEGIN PRIVATE KEY-----\n-----END PRIVATE KEY-----\n",
            "token_uri": "https://oauth2.googleapis.com/token",
        })
        .to_string(),
        "XMTP_S3_ENDPOINT" => "https://s3.us-east-1.amazonaws.com".into(),
        "XMTP_S3_ACCESS_KEY_ID" => "AKIAIOSFODNN7EXAMPLE".into(),
        "XMTP_S3_SECRET_ACCESS_KEY" => "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into(),
        "XMTP_S3_SESSION_TOKEN" => "example-session-token".into(),
        other => panic!("the example references {other}; give it a value in this test"),
    }
}

/// The names of every environment reference in an example file, including
/// the commented alternatives.
fn referenced_variables(example: &str) -> BTreeSet<&str> {
    example
        .split("\"env:")
        .skip(1)
        .map(|rest| &rest[..rest.find('"').expect("a reference ends its string")])
        .collect()
}

/// The field table lists exactly the schema's paths. The example names every
/// one of them, and loads as written and with each commented alternative.
#[xmtp_common::test(unwrap_try = true)]
fn the_spec_example_loads_and_names_every_key() {
    if std::env::var_os(CHILD).is_some() {
        for (label, document) in example_documents(example()) {
            if let Err(error) = Config::load_str(&document) {
                panic!("{label} does not load: {error}");
            }
        }
        return;
    }

    let schema = schema_paths();
    let table: BTreeSet<String> = field_table()
        .iter()
        .map(|row| key_pattern(&row.key))
        .collect();
    assert_eq!(
        table, schema,
        "the configuration field table and the schema list different keys"
    );

    let documents = example_documents(example());
    let mut named = BTreeSet::new();
    for (label, document) in &documents {
        let parsed: toml::Table =
            toml::from_str(document).unwrap_or_else(|error| panic!("{label} is not TOML: {error}"));
        document_paths(&parsed, "", &schema, &mut named);
    }
    let unnamed: Vec<&String> = schema.difference(&named).collect();
    assert!(unnamed.is_empty(), "the example does not name {unnamed:?}");

    let mut kinds: BTreeSet<String> = documents
        .iter()
        .filter_map(|(label, _)| label.strip_prefix("credentials kind = "))
        .map(|kind| kind.trim_matches('"').to_owned())
        .collect();
    let active = example()
        .lines()
        .find_map(|line| line.strip_prefix("kind = "))
        .expect("the example sets a credential kind");
    assert!(kinds.insert(active.trim_matches('"').to_owned()));
    assert_eq!(kinds, schema_credential_kinds());

    let mut command = child_test(module_path!(), "the_spec_example_loads_and_names_every_key");
    command.env(CHILD, "1");
    for name in referenced_variables(example()) {
        command.env(name, variable(name));
    }
    assert_one_test_passed(&command.output()?);
}

/// The example file of each release, copied from CONF section 1.3 as that
/// release published it. A copy never changes once committed.
const RELEASES: &[(&str, &str)] = &[("1.12.0", include_str!("releases/1.12.0.toml"))];

/// The example file of every earlier release still loads, as written and with
/// each of its commented alternatives.
// verifies: CONF-086
#[xmtp_common::test(unwrap_try = true)]
fn every_release_example_still_loads() {
    if std::env::var_os(CHILD).is_some() {
        for (release, example) in RELEASES {
            for (label, document) in example_documents(example) {
                if let Err(error) = Config::load_str(&document) {
                    panic!("{label} of release {release} does not load: {error}");
                }
            }
        }
        return;
    }
    let mut command = child_test(module_path!(), "every_release_example_still_loads");
    command.env(CHILD, "1");
    for (_, example) in RELEASES {
        for name in referenced_variables(example) {
            command.env(name, variable(name));
        }
    }
    assert_one_test_passed(&command.output()?);
}
