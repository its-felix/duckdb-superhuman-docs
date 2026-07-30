use super::*;
use std::future::Future;

fn run<T>(future: impl Future<Output = Result<T, String>>) -> Result<T, String> {
    crate::platform::block_on_result(future)
}

#[test]
fn browser_urls_expose_embedded_doc_ids() {
    assert!(is_browser_url(
        "https://coda.io/d/Launch-Status_dAbCDeFGH/Page_su123"
    ));
    assert!(is_browser_url("http://localhost:8080/d/Test_dmock-doc"));
    assert!(!is_browser_url("AbCDeFGH"));
    assert_eq!(
        doc_id_from_browser_url("https://coda.io/d/Launch-Status_dAbCDeFGH/Page_su123"),
        Some("AbCDeFGH".to_string())
    );
    assert_eq!(
        doc_id_from_browser_url("https://example.com/published/launch-status"),
        None
    );
}

#[test]
fn attach_resource_prefixes_are_stripped() {
    for prefix in [
        "coda:",
        "superhuman:",
        "superhuman-docs:",
        "superhuman_docs:",
    ] {
        assert_eq!(
            strip_attach_resource_prefix(&format!("{prefix}https://coda.io/d/_dDoc")),
            "https://coda.io/d/_dDoc"
        );
    }
    assert_eq!(strip_attach_resource_prefix("doc-id"), "doc-id");
}

#[test]
fn resolved_links_map_doc_resources_to_their_containing_doc() {
    assert_eq!(
        doc_id_from_resolved_link(
            r#"{"resource":{"type":"doc","id":"doc-1","href":"https://coda.io/apis/v1/docs/doc-1"}}"#
        )
        .unwrap(),
        "doc-1"
    );
    assert_eq!(
        doc_id_from_resolved_link(
            r#"{"resource":{"type":"table","id":"table-1","href":"https://coda.io/apis/v1/docs/doc-2/tables/table-1"}}"#
        )
        .unwrap(),
        "doc-2"
    );
    let error = doc_id_from_resolved_link(
        r#"{"resource":{"type":"folder","id":"folder-1","href":"https://coda.io/apis/v1/folders/folder-1"}}"#,
    )
    .unwrap_err();
    assert!(error.contains("not contained by a document"));
    assert!(doc_id_from_resolved_link("not-json")
        .unwrap_err()
        .contains("invalid ResolveBrowserLink response"));
}

#[derive(Default)]
struct TestAttachHostContext {
    options: HashMap<String, String>,
    secrets: HashMap<String, String>,
}

unsafe extern "C" fn test_attach_get_option(
    userdata: *mut c_void,
    name: *const c_char,
    out: *mut RustExtString,
    _err: *mut RustExtError,
) -> bool {
    let context = unsafe { &*(userdata.cast::<TestAttachHostContext>()) };
    let name = unsafe { CStr::from_ptr(name) }.to_string_lossy();
    let value = context
        .options
        .get(name.as_ref())
        .map(String::as_str)
        .unwrap_or("");
    unsafe { out.write(alloc_string(value)) };
    true
}

unsafe extern "C" fn test_attach_lookup_secret(
    userdata: *mut c_void,
    scope: RustExtString,
    _secret_type: *const c_char,
    _secret_key: *const c_char,
    out: *mut RustExtString,
    _err: *mut RustExtError,
) -> bool {
    let context = unsafe { &*(userdata.cast::<TestAttachHostContext>()) };
    let value = context
        .secrets
        .get(scope.as_str())
        .map(String::as_str)
        .unwrap_or("");
    unsafe { out.write(alloc_string(value)) };
    true
}

fn test_attach_host() -> RustExtAttachHost {
    RustExtAttachHost {
        get_option: test_attach_get_option,
        lookup_secret: test_attach_lookup_secret,
    }
}

fn inspect_attach_config<T>(config: RustExtAttachConfig, inspect: T)
where
    T: FnOnce(&SuperhumanDocsClientConfig),
{
    let inner = unsafe { &*config.handle.cast::<SuperhumanDocsClientConfig>() };
    inspect(inner);
    crate::exports::rust_ext_free_attach_config(config);
}

#[test]
fn attach_mutation_options_have_defaults_and_parse_explicit_values() {
    let host = test_attach_host();
    let mut defaults = TestAttachHostContext::default();
    defaults
        .options
        .insert("token".to_string(), "explicit-token".to_string());
    let config = run(resolve_attach(
        borrow_string("mock-doc"),
        &host,
        (&mut defaults as *mut TestAttachHostContext).cast(),
    ))
    .unwrap();
    inspect_attach_config(config, |config| {
        assert!(!config.wait_for_mutations);
        assert_eq!(config.mutation_timeout_seconds, 60);
        assert!(!config.allow_mutation_warnings);
    });

    let mut explicit = TestAttachHostContext::default();
    explicit
        .options
        .insert("token".to_string(), "explicit-token".to_string());
    explicit
        .options
        .insert("wait_for_mutations".to_string(), "true".to_string());
    explicit
        .options
        .insert("mutation_timeout_seconds".to_string(), "17".to_string());
    explicit
        .options
        .insert("allow_mutation_warnings".to_string(), "true".to_string());
    let config = run(resolve_attach(
        borrow_string("mock-doc"),
        &host,
        (&mut explicit as *mut TestAttachHostContext).cast(),
    ))
    .unwrap();
    inspect_attach_config(config, |config| {
        assert!(config.wait_for_mutations);
        assert_eq!(config.mutation_timeout_seconds, 17);
        assert!(config.allow_mutation_warnings);
    });
}

#[test]
fn browser_url_resolution_supports_scoped_general_and_explicit_credentials() {
    let _network_guard = NETWORK_UNIT_TEST_LOCK.lock().unwrap();
    let host = test_attach_host();

    let scoped_server = MockSuperhumanDocsServer::start();
    let mut scoped = TestAttachHostContext::default();
    scoped
        .options
        .insert("api_base".to_string(), scoped_server.base_url());
    scoped.secrets.insert(
        "superhuman_docs:mock-doc".to_string(),
        "scoped-token".to_string(),
    );
    let config = run(resolve_attach(
        borrow_string("https://coda.io/d/Mock_dmock-doc/Page_su123"),
        &host,
        (&mut scoped as *mut TestAttachHostContext).cast(),
    ))
    .unwrap();
    inspect_attach_config(config, |config| {
        assert_eq!(config.resource, "mock-doc");
        assert_eq!(config.credential, "scoped-token");
    });
    assert!(scoped_server.requests().iter().all(|request| request
        .headers
        .lines()
        .any(|line| line.eq_ignore_ascii_case("Authorization: Bearer scoped-token"))));
    drop(scoped_server);

    let general_server = MockSuperhumanDocsServer::start();
    let mut general = TestAttachHostContext::default();
    general
        .options
        .insert("api_base".to_string(), general_server.base_url());
    general
        .secrets
        .insert("superhuman_docs:".to_string(), "general-token".to_string());
    general.secrets.insert(
        "superhuman_docs:mock-doc".to_string(),
        "canonical-token".to_string(),
    );
    let config = run(resolve_attach(
        borrow_string("https://example.com/published/launch-status"),
        &host,
        (&mut general as *mut TestAttachHostContext).cast(),
    ))
    .unwrap();
    inspect_attach_config(config, |config| {
        assert_eq!(config.resource, "mock-doc");
        assert_eq!(config.credential, "canonical-token");
    });
    assert!(general_server.requests().iter().all(|request| request
        .headers
        .lines()
        .any(|line| line.eq_ignore_ascii_case("Authorization: Bearer general-token"))));
    drop(general_server);

    let explicit_server = MockSuperhumanDocsServer::start();
    let mut explicit = TestAttachHostContext::default();
    explicit
        .options
        .insert("api_base".to_string(), explicit_server.base_url());
    explicit
        .options
        .insert("token".to_string(), "explicit-token".to_string());
    explicit.secrets.insert(
        "superhuman_docs:mock-doc".to_string(),
        "ignored-token".to_string(),
    );
    let config = run(resolve_attach(
        borrow_string("https://coda.io/d/Mock_dmock-doc"),
        &host,
        (&mut explicit as *mut TestAttachHostContext).cast(),
    ))
    .unwrap();
    inspect_attach_config(config, |config| {
        assert_eq!(config.resource, "mock-doc");
        assert_eq!(config.credential, "explicit-token");
    });
}

#[test]
fn noncanonical_browser_url_without_bootstrap_credential_is_targeted_error() {
    let host = test_attach_host();
    let mut context = TestAttachHostContext::default();
    let error = match run(resolve_attach(
        borrow_string("https://example.com/published/launch-status"),
        &host,
        (&mut context as *mut TestAttachHostContext).cast(),
    )) {
        Ok(config) => {
            crate::exports::rust_ext_free_attach_config(config);
            panic!("browser URL without a bootstrap credential unexpectedly resolved")
        }
        Err(error) => error,
    };
    assert!(error.contains("browser URL attachment requires TOKEN"));
    assert!(error.contains("general superhuman_docs secret"));
}

#[test]
fn secret_policy_is_implemented_by_rust_callback() {
    let result = run(create_secret(RustExtSecretCreateInput {
        secret_type: borrow_string("superhuman_docs"),
        provider: borrow_string("config"),
        name: borrow_string("test"),
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(result.scope_count, 1);
    assert_eq!(unsafe { &*result.scope }.as_str(), "superhuman_docs:");
    assert_eq!(result.entry_count, 0);
    assert_eq!(result.redact_key_count, 1);
    assert_eq!(unsafe { &*result.redact_keys }.as_str(), "token");
    free_secret(result);

    let option = RustExtNamedValue {
        name: borrow_string("unsupported"),
        value: RustExtInputValue {
            value_type: 5,
            string_value: borrow_string("value"),
            ..Default::default()
        },
    };
    let error = match run(create_secret(RustExtSecretCreateInput {
        secret_type: borrow_string("superhuman_docs"),
        provider: borrow_string("config"),
        name: borrow_string("test"),
        options: &option,
        option_count: 1,
        ..Default::default()
    })) {
        Ok(result) => {
            free_secret(result);
            panic!("unsupported secret parameter unexpectedly succeeded")
        }
        Err(error) => error,
    };
    assert_eq!(
        error,
        "Unknown named parameter for superhuman_docs secret: unsupported"
    );
}
