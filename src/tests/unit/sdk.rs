use super::*;

#[test]
fn sdk_uses_the_superhuman_docs_api_by_default() {
    assert_eq!(DEFAULT_BASE_URL, "https://docs.superhuman.com/apis/v1");
}

#[test]
fn token_validation_uses_whoami_status() {
    let _network_guard = NETWORK_UNIT_TEST_LOCK.lock().unwrap();
    let server = MockSuperhumanDocsServer::start();
    crate::platform::block_on_result(validate_token_at(&server.base_url(), "mock-token")).unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].path, "/whoami");
    assert!(
        requests[0]
            .headers
            .lines()
            .any(|line| line.eq_ignore_ascii_case("Authorization: Bearer mock-token")),
        "expected bearer token in request headers: {}",
        requests[0].headers
    );
    drop(server);

    let server = MockSuperhumanDocsServer::start_with_whoami_status("401 Unauthorized");
    let error =
        crate::platform::block_on_result(validate_token_at(&server.base_url(), "bad-token"))
            .unwrap_err();
    assert_eq!(
        error,
        "Whoami returned HTTP 401, expected 200: not valid JSON"
    );
}

#[test]
fn native_transport_preserves_methods_bodies_headers_and_non_success_responses() {
    let _network_guard = NETWORK_UNIT_TEST_LOCK.lock().unwrap();
    let server = MockSuperhumanDocsServer::start();
    let sdk = SdkClient::at(&server.base_url(), "mock-token").unwrap();
    let cases = [
        (superhuman_docs_async::Method::Get, "get", None),
        (
            superhuman_docs_async::Method::Post,
            "post",
            Some(br#"{"kind":"post"}"#.to_vec()),
        ),
        (
            superhuman_docs_async::Method::Patch,
            "patch",
            Some(br#"{"kind":"patch"}"#.to_vec()),
        ),
        (
            superhuman_docs_async::Method::Delete,
            "delete",
            Some(br#"{"rowIds":["r1","r2"]}"#.to_vec()),
        ),
    ];
    for (method, path, body) in cases {
        let expected_body = body.clone().unwrap_or_default();
        let response = crate::platform::block_on(sdk.send_raw(superhuman_docs_async::Request {
            operation: "TransportTest",
            method,
            url: format!("{}/transport/{path}", server.base_url()),
            body,
            expected_status: 200,
        }))
        .unwrap()
        .unwrap();
        assert_eq!(response.status, 404);
        assert!(!response.body.is_empty());

        let requests = server.requests();
        let request = requests.last().unwrap();
        assert_eq!(request.method, method.as_str());
        assert_eq!(request.path, format!("/transport/{path}"));
        assert_eq!(request.body.as_bytes(), expected_body);
        assert!(request
            .headers
            .lines()
            .any(|line| line.eq_ignore_ascii_case("Authorization: Bearer mock-token")));
        assert!(request
            .headers
            .lines()
            .any(|line| line.eq_ignore_ascii_case("Content-Type: application/json")));
        assert_eq!(requests.len(), path_index(path) + 1);
    }
}

#[test]
fn native_transport_preserves_empty_response_bodies() {
    let _network_guard = NETWORK_UNIT_TEST_LOCK.lock().unwrap();
    let (base_url, attempts, server) = raw_response_server(RawServerBehavior::Respond(
        b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
    ));
    let sdk = SdkClient::at(&base_url, "mock-token").unwrap();
    let response = crate::platform::block_on(sdk.send_raw(superhuman_docs_async::Request {
        operation: "EmptyResponseTest",
        method: superhuman_docs_async::Method::Get,
        url: format!("{base_url}/empty"),
        body: None,
        expected_status: 204,
    }))
    .unwrap()
    .unwrap();

    server.join().unwrap();
    assert_eq!(response.status, 204);
    assert!(response.body.is_empty());
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}

#[test]
fn native_transport_reports_malformed_http_without_retrying() {
    let _network_guard = NETWORK_UNIT_TEST_LOCK.lock().unwrap();
    let (base_url, attempts, server) = raw_response_server(RawServerBehavior::Respond(
        b"this is not an HTTP response\r\n\r\n".to_vec(),
    ));
    let sdk = SdkClient::at(&base_url, "mock-token").unwrap();
    let error = crate::platform::block_on(sdk.send_raw(superhuman_docs_async::Request {
        operation: "MalformedResponseTest",
        method: superhuman_docs_async::Method::Get,
        url: format!("{base_url}/malformed"),
        body: None,
        expected_status: 200,
    }))
    .unwrap()
    .unwrap_err();

    server.join().unwrap();
    assert!(matches!(error, superhuman_docs_async::Error::Transport(_)));
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}

#[test]
fn native_transport_times_out_without_retrying() {
    let _network_guard = NETWORK_UNIT_TEST_LOCK.lock().unwrap();
    let (base_url, attempts, server) =
        raw_response_server(RawServerBehavior::Stall(Duration::from_millis(100)));
    let sdk =
        SdkClient::at_with_timeout(&base_url, "mock-token", Duration::from_millis(20)).unwrap();
    let error = crate::platform::block_on(sdk.send_raw(superhuman_docs_async::Request {
        operation: "TimeoutTest",
        method: superhuman_docs_async::Method::Get,
        url: format!("{base_url}/timeout"),
        body: None,
        expected_status: 200,
    }))
    .unwrap()
    .unwrap_err();

    server.join().unwrap();
    assert!(matches!(error, superhuman_docs_async::Error::Transport(_)));
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}

#[test]
fn generated_bulk_delete_sends_json_body_on_delete() {
    let _network_guard = NETWORK_UNIT_TEST_LOCK.lock().unwrap();
    let server = MockSuperhumanDocsServer::start();
    let sdk = SdkClient::at(&server.base_url(), "mock-token").unwrap();

    crate::platform::block_on_result(sdk.execute(|client| {
        Box::pin(async move {
            client
                .tables()
                .rows()
                .delete_rows(superhuman_docs_async::operations::DeleteRowsInput {
                    doc_id: "mock-doc".to_string(),
                    table_id_or_name: "tbl1".to_string(),
                    payload: superhuman_docs_async::operations::RowsDelete {
                        row_ids: vec!["r1".to_string(), "r2".to_string()],
                    },
                })
                .await
        })
    }))
    .unwrap();

    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "DELETE");
    assert_eq!(requests[0].path, "/docs/mock-doc/tables/tbl1/rows");
    assert_eq!(requests[0].body, r#"{"rowIds":["r1","r2"]}"#);
}

enum RawServerBehavior {
    Respond(Vec<u8>),
    Stall(Duration),
}

fn raw_response_server(
    behavior: RawServerBehavior,
) -> (
    String,
    Arc<std::sync::atomic::AtomicUsize>,
    thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let server_attempts = Arc::clone(&attempts);
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        serve_raw_connection(stream, &behavior, &server_attempts);
        listener.set_nonblocking(true).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_millis(150);
        while std::time::Instant::now() < deadline {
            match listener.accept() {
                Ok((stream, _)) => serve_raw_connection(stream, &behavior, &server_attempts),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("raw response server failed: {error}"),
            }
        }
    });
    (format!("http://{address}"), attempts, server)
}

fn serve_raw_connection(
    mut stream: TcpStream,
    behavior: &RawServerBehavior,
    attempts: &std::sync::atomic::AtomicUsize,
) {
    attempts.fetch_add(1, Ordering::SeqCst);
    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    let mut request = Vec::new();
    let mut buffer = [0; 1024];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => request.extend_from_slice(&buffer[..count]),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                break;
            }
            Err(error) => panic!("raw response server read failed: {error}"),
        }
    }
    match behavior {
        RawServerBehavior::Respond(response) => stream.write_all(response).unwrap(),
        RawServerBehavior::Stall(duration) => thread::sleep(*duration),
    }
}

fn path_index(path: &str) -> usize {
    match path {
        "get" => 0,
        "post" => 1,
        "patch" => 2,
        "delete" => 3,
        _ => unreachable!(),
    }
}

#[test]
fn token_environment_variable_is_read_eagerly() {
    let name = format!(
        "DUCKDB_SUPERHUMAN_DOCS_TOKEN_ENV_TEST_{}",
        std::process::id()
    );
    env::set_var(&name, "resolved-token");
    assert_eq!(read_environment_variable(&name).unwrap(), "resolved-token");
    env::remove_var(&name);
    assert!(read_environment_variable(&name).is_err());
}
