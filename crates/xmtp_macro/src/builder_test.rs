use crate::builder::*;
use quote::quote;
use syn::ItemStruct;

#[test]
fn test_to_camel_case() {
    assert_eq!(to_camel_case("my_field_name"), "myFieldName");
    assert_eq!(to_camel_case("desc"), "desc");
    assert_eq!(to_camel_case("a_b_c"), "aBC");
    assert_eq!(to_camel_case("already"), "already");
    // Leading underscores should not capitalize the first real character
    assert_eq!(to_camel_case("_my_field"), "myField");
    assert_eq!(to_camel_case("__double"), "double");
}

/// No-op config for testing the builder logic without any binding framework.
fn test_config() -> AnnotationConfig {
    AnnotationConfig {
        struct_ann: quote! {},
        impl_ann: quote! {},
        constructor_ann: quote! {},
        setter_ann: |_| quote! {},
        setter_impl_ann: None,
        setter_style: SetterStyle::MutRefChain,
        setter_prefix: "",
    }
}

/// Helper: parse a struct, expand with no-op config, and verify the output
/// is syntactically valid Rust.
fn expand_and_parse(input: &str) -> syn::Result<proc_macro2::TokenStream> {
    let item: ItemStruct = syn::parse_str(input)?;
    expand_builder(item, test_config())
}

#[test]
fn test_required_field_in_constructor() {
    let output = expand_and_parse(
        r#"
        pub struct Foo {
            #[builder(required)]
            pub name: String,
        }
        "#,
    )
    .unwrap();
    let output_str = output.to_string();
    assert!(output_str.contains("fn new (name : String)"));
    assert!(!output_str.contains("fn name (& mut self"));
}

#[test]
fn test_optional_field_setter() {
    let output = expand_and_parse(
        r#"
        pub struct Foo {
            #[builder(required)]
            pub id: u32,
            pub desc: Option<String>,
        }
        "#,
    )
    .unwrap();
    let output_str = output.to_string();
    assert!(output_str.contains("fn desc (& mut self , desc : String)"));
    assert!(output_str.contains("desc : None"));
}

#[test]
fn test_default_field() {
    let output = expand_and_parse(
        r#"
        pub struct Foo {
            #[builder(required)]
            pub id: u32,
            #[builder(default = "42")]
            pub count: u32,
        }
        "#,
    )
    .unwrap();
    let output_str = output.to_string();
    assert!(output_str.contains("fn count (& mut self , count : u32)"));
    assert!(output_str.contains("count : 42"));
}

#[test]
fn test_skip_field() {
    let output = expand_and_parse(
        r#"
        pub struct Foo {
            #[builder(required)]
            pub id: u32,
            #[builder(skip)]
            internal: Vec<u8>,
        }
        "#,
    )
    .unwrap();
    let output_str = output.to_string();
    assert!(!output_str.contains("fn internal"));
    assert!(output_str.contains("internal : Default :: default ()"));
}

#[test]
fn test_optional_must_be_option_type() {
    let result = expand_and_parse(
        r#"
        pub struct Foo {
            #[builder(optional)]
            pub bad: String,
        }
        "#,
    );
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Option<T>"));
}

#[test]
fn test_missing_builder_attribute() {
    let result = expand_and_parse(
        r#"
        pub struct Foo {
            pub bare: String,
        }
        "#,
    );
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("#[builder("));
}

#[test]
fn test_multiple_required_fields() {
    let output = expand_and_parse(
        r#"
        pub struct Foo {
            #[builder(required)]
            pub a: u32,
            #[builder(required)]
            pub b: String,
        }
        "#,
    )
    .unwrap();
    let output_str = output.to_string();
    assert!(output_str.contains("fn new (a : u32 , b : String)"));
}

#[test]
fn test_all_field_modes_together() {
    let output = expand_and_parse(
        r#"
        pub struct Builder {
            #[builder(required)]
            pub name: String,
            pub desc: Option<String>,
            #[builder(default = "true")]
            pub enabled: bool,
            #[builder(skip)]
            internal: u64,
        }
        "#,
    )
    .unwrap();
    let output_str = output.to_string();
    assert!(output_str.contains("fn new (name : String)"));
    assert!(output_str.contains("fn desc (& mut self , desc : String)"));
    assert!(output_str.contains("fn enabled (& mut self , enabled : bool)"));
    assert!(!output_str.contains("fn internal"));
}

#[test]
fn test_annotations_are_applied() {
    let item: ItemStruct = syn::parse_str(
        r#"
        pub struct Foo {
            #[builder(required)]
            pub id: u32,
            pub name: Option<String>,
        }
        "#,
    )
    .unwrap();

    let config = AnnotationConfig {
        struct_ann: quote! { #[my_struct_attr] },
        impl_ann: quote! { #[my_impl_attr] },
        constructor_ann: quote! { #[my_ctor_attr] },
        setter_ann: |ident| {
            let name = ident.to_string();
            quote! { #[my_setter(name = #name)] }
        },
        setter_impl_ann: None,
        setter_style: SetterStyle::MutRefChain,
        setter_prefix: "",
    };

    let output = expand_builder(item, config).unwrap();
    let output_str = output.to_string();

    assert!(output_str.contains("my_struct_attr"));
    assert!(output_str.contains("my_impl_attr"));
    assert!(output_str.contains("my_ctor_attr"));
    assert!(output_str.contains("my_setter"));
}

#[test]
fn test_setter_prefix() {
    let item: ItemStruct = syn::parse_str(
        r#"
        pub struct Foo {
            #[builder(required)]
            pub id: u32,
            pub name: Option<String>,
        }
        "#,
    )
    .unwrap();

    let config = AnnotationConfig {
        struct_ann: quote! {},
        impl_ann: quote! {},
        constructor_ann: quote! {},
        setter_ann: |ident| {
            let name = ident.to_string();
            quote! { #[setter(js_name = #name)] }
        },
        setter_impl_ann: None,
        setter_style: SetterStyle::MutRefChain,
        setter_prefix: "set_",
    };

    let output = expand_builder(item, config).unwrap();
    let output_str = output.to_string();
    // Method is named `set_name`, not `name`.
    assert!(output_str.contains("fn set_name (& mut self , name : String)"));
    // The annotation callback receives the prefixed name.
    assert!(output_str.contains("js_name = \"set_name\""));
}

#[test]
fn test_duplicate_builder_attribute_rejected() {
    let result = expand_and_parse(
        r#"
        pub struct Foo {
            #[builder(required)]
            #[builder(optional)]
            pub id: Option<u32>,
        }
        "#,
    );
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("duplicate"));
}

#[test]
fn test_non_builder_attrs_preserved() {
    let output = expand_and_parse(
        r#"
        pub struct Foo {
            #[builder(required)]
            #[serde(rename = "ID")]
            pub id: u32,
        }
        "#,
    )
    .unwrap();
    let output_str = output.to_string();
    assert!(output_str.contains("serde"));
}

#[test]
fn test_generic_struct_with_bounds() {
    let output = expand_and_parse(
        r#"
        pub struct Foo<T: Clone> {
            #[builder(required)]
            pub value: T,
        }
        "#,
    )
    .unwrap();
    let output_str = output.to_string();
    // impl block should have bounds, type position should not
    assert!(output_str.contains("impl < T : Clone > Foo < T >"));
}

#[test]
fn test_implicit_optional_for_option_type() {
    let output = expand_and_parse(
        r#"
        pub struct Foo {
            #[builder(required)]
            pub id: u32,
            pub tag: Option<String>,
        }
        "#,
    )
    .unwrap();
    let output_str = output.to_string();
    // Inferred optional: setter takes inner type, initialised to None.
    assert!(output_str.contains("fn tag (& mut self , tag : String)"));
    assert!(output_str.contains("tag : None"));
}

#[test]
fn test_explicit_optional_still_works() {
    let output = expand_and_parse(
        r#"
        pub struct Foo {
            #[builder(required)]
            pub id: u32,
            #[builder(optional)]
            pub tag: Option<String>,
        }
        "#,
    )
    .unwrap();
    let output_str = output.to_string();
    assert!(output_str.contains("fn tag (& mut self , tag : String)"));
    assert!(output_str.contains("tag : None"));
}
