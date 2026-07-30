mod responses;

use responses::mock_response;

use super::*;

#[derive(Clone, Debug)]
pub(super) struct MockRequest {
    pub(super) method: String,
    pub(super) path: String,
    pub(super) query: String,
    pub(super) headers: String,
    pub(super) body: String,
}

pub(super) struct MockSuperhumanDocsServer {
    address: String,
    requests: Arc<Mutex<Vec<MockRequest>>>,
    shutdown: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl MockSuperhumanDocsServer {
    pub(super) fn start() -> Self {
        Self::start_with_whoami_status("200 OK")
    }

    pub(super) fn start_with_whoami_status(whoami_status: &'static str) -> Self {
        let listener =
            TcpListener::bind("127.0.0.1:0").expect("failed to bind mock Superhuman Docs server");
        let address = listener
            .local_addr()
            .expect("failed to read mock Superhuman Docs server address")
            .to_string();
        listener
            .set_nonblocking(true)
            .expect("failed to configure mock Superhuman Docs server");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_requests = Arc::clone(&requests);
        let thread_shutdown = Arc::clone(&shutdown);
        let handle = thread::spawn(move || loop {
            if thread_shutdown.load(Ordering::SeqCst) {
                break;
            }
            match listener.accept() {
                Ok((stream, _)) => {
                    stream
                        .set_nonblocking(false)
                        .expect("failed to configure mock Superhuman Docs connection");
                    handle_mock_connection(stream, &thread_requests, whoami_status)
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(err) => {
                    panic!("mock Superhuman Docs server failed to accept connection: {err}")
                }
            }
        });
        Self {
            address,
            requests,
            shutdown,
            handle: Some(handle),
        }
    }

    pub(super) fn base_url(&self) -> String {
        format!("http://{}", self.address)
    }

    pub(super) fn requests(&self) -> Vec<MockRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for MockSuperhumanDocsServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(&self.address);
        if let Some(handle) = self.handle.take() {
            handle
                .join()
                .expect("mock Superhuman Docs server thread panicked");
        }
    }
}

fn handle_mock_connection(
    mut stream: TcpStream,
    requests: &Arc<Mutex<Vec<MockRequest>>>,
    whoami_status: &'static str,
) {
    let mut buffer = Vec::new();
    let mut temp = [0; 1024];
    let header_end;
    loop {
        let read = stream
            .read(&mut temp)
            .expect("failed to read mock Superhuman Docs request");
        if read == 0 {
            return;
        }
        buffer.extend_from_slice(&temp[..read]);
        if let Some(position) = find_header_end(&buffer) {
            header_end = position;
            break;
        }
    }

    let headers = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    let body_start = header_end + 4;
    while buffer.len() < body_start + content_length {
        let read = stream
            .read(&mut temp)
            .expect("failed to read mock Superhuman Docs request body");
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&temp[..read]);
    }

    let request_line = headers.lines().next().unwrap_or("");
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or("").to_string();
    let target = request_parts.next().unwrap_or("");
    let (path, query) = target
        .split_once('?')
        .map(|(path, query)| (path.to_string(), query.to_string()))
        .unwrap_or_else(|| (target.to_string(), String::new()));
    let body =
        String::from_utf8_lossy(&buffer[body_start..body_start + content_length]).to_string();
    let request_occurrence = {
        let mut requests = requests.lock().unwrap();
        requests.push(MockRequest {
            method: method.clone(),
            path: path.clone(),
            query: query.clone(),
            headers,
            body: body.clone(),
        });
        requests
            .iter()
            .filter(|request| request.method == method && request.path == path)
            .count()
    };

    let (status, response_body) = mock_response(
        &method,
        &path,
        &query,
        &body,
        request_occurrence,
        whoami_status,
    );
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response_body.len(),
        response_body
    );
    stream
        .write_all(response.as_bytes())
        .expect("failed to write mock Superhuman Docs response");
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}
