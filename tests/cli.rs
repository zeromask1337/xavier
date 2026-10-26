use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde_json::Value;

const FIXTURE: &str = include_str!("fixtures/response.json");

#[derive(Debug, Clone)]
struct RequestRecord {
    path: String,
    authorization: Option<String>,
    body: String,
}

struct TestServer {
    addr: SocketAddr,
    requests: Arc<Mutex<Vec<RequestRecord>>>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl TestServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        listener
            .set_nonblocking(true)
            .expect("nonblocking listener");
        let addr = listener.local_addr().expect("server address");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));

        let mut stub: Value = serde_json::from_str(FIXTURE).expect("fixture");
        stub["output"][2]["content"][0]["text"] =
            Value::String(r#"{"summary":"placeholder","posts":[]}"#.to_string());
        let fixture = Arc::new(FIXTURE.to_string());
        let stub = Arc::new(stub.to_string());

        let thread_requests = Arc::clone(&requests);
        let thread_stop = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let requests = Arc::clone(&thread_requests);
                        let fixture = Arc::clone(&fixture);
                        let stub = Arc::clone(&stub);
                        thread::spawn(move || {
                            let _ = handle_connection(stream, requests, fixture, stub);
                        });
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            addr,
            requests,
            stop,
            handle: Some(handle),
        }
    }

    fn base_url(&self) -> String {
        format!("http://{}/v1", self.addr)
    }

    fn requests(&self) -> Vec<RequestRecord> {
        self.requests.lock().expect("request lock").clone()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn handle_connection(
    mut stream: TcpStream,
    requests: Arc<Mutex<Vec<RequestRecord>>>,
    fixture: Arc<String>,
    stub: Arc<String>,
) -> std::io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;

    let mut buffer = Vec::new();
    let mut header_end = None;
    let mut chunk = [0_u8; 4096];
    while header_end.is_none() {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        header_end = find_subsequence(&buffer, b"\r\n\r\n").map(|index| index + 4);
        if buffer.len() > 2 * 1024 * 1024 {
            break;
        }
    }
    let Some(header_end) = header_end else {
        return Ok(());
    };

    let header_text = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_string();

    let mut authorization = None;
    let mut content_length = 0_usize;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            let value = value.trim();
            if name.eq_ignore_ascii_case("authorization") {
                authorization = Some(value.to_string());
            } else if name.eq_ignore_ascii_case("content-length") {
                content_length = value.parse().unwrap_or(0);
            }
        }
    }

    while buffer.len() < header_end + content_length {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    let body = String::from_utf8_lossy(&buffer[header_end..]).to_string();

    requests.lock().expect("request lock").push(RequestRecord {
        path: path.clone(),
        authorization,
        body,
    });

    let (status, content_type, payload) = match path.as_str() {
        "/v1/responses" => (200, "application/json", fixture.as_str()),
        "/v1/stub/responses" => (200, "application/json", stub.as_str()),
        "/v1/unauthorized/responses" => (
            401,
            "application/json",
            r#"{"error":{"message":"invalid key"}}"#,
        ),
        _ => (404, "text/plain", "not found"),
    };
    let reason = match status {
        200 => "OK",
        401 => "Unauthorized",
        _ => "Not Found",
    };

    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        payload.len()
    )?;
    stream.write_all(payload.as_bytes())?;
    Ok(())
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn command(server: &TestServer) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_xavier"));
    command
        .env_remove("XAI_API_KEY")
        .env_remove("XAI_BASE_URL")
        .env_remove("XAVIER_MODEL")
        .env("XAI_API_KEY", "test-key")
        .env("XAI_BASE_URL", server.base_url())
        .stdin(Stdio::null());
    command
}

fn run_cli(server: &TestServer, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = command(server);
    command.args(args);
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().expect("run xavier")
}

fn run_cli_stdin(server: &TestServer, args: &[&str], input: &str) -> Output {
    let mut child = command(server)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn xavier");
    child
        .stdin
        .as_mut()
        .expect("stdin pipe")
        .write_all(input.as_bytes())
        .expect("write stdin");
    drop(child.stdin.take());
    child.wait_with_output().expect("wait for xavier")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

#[test]
fn renders_agent_first_markdown_by_default() {
    let server = TestServer::start();
    let output = run_cli(&server, &["grok 4.7", "--limit", "5"], &[]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(stderr(&output).is_empty());

    let text = stdout(&output);
    assert!(text.contains("# X search: grok 4.7"));
    assert!(text.contains("model=grok-4.7"));
    assert!(text.contains("## Posts"));
    assert!(text.contains("https://x.com/alice/status/111"));

    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/v1/responses");
    assert_eq!(
        requests[0].authorization.as_deref(),
        Some("Bearer test-key")
    );
    let body: Value = serde_json::from_str(&requests[0].body).expect("request body");
    assert_eq!(body["model"], "grok-4.7");
    assert_eq!(body["tools"][0]["type"], "x_search");
}

#[test]
fn emits_structured_json() {
    let server = TestServer::start();
    let output = run_cli(&server, &["grok 4.7", "--json"], &[]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));

    let parsed: Value = serde_json::from_str(&stdout(&output)).expect("json output");
    assert_eq!(parsed["posts"].as_array().unwrap().len(), 2);
    assert_eq!(parsed["meta"]["usage"]["cost_usd"], 0.001234);
    assert_eq!(parsed["meta"]["warnings"].as_array().unwrap().len(), 1);
}

#[test]
fn emits_jsonl() {
    let server = TestServer::start();
    let output = run_cli(&server, &["grok 4.7", "--jsonl"], &[]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));

    let text = stdout(&output);
    let lines: Vec<&str> = text.trim_end().lines().collect();
    assert_eq!(lines.len(), 3);
    let head: Value = serde_json::from_str(lines[0]).unwrap();
    let post: Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(head["type"], "result");
    assert_eq!(post["type"], "post");
}

#[test]
fn passes_raw_payloads_through_without_validation() {
    let server = TestServer::start();
    let output = run_cli(&server, &["grok 4.7", "--raw"], &[]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let parsed: Value = serde_json::from_str(&stdout(&output)).unwrap();
    assert_eq!(parsed["id"], "resp_test_123");
}

#[test]
fn prints_the_request_body_on_dry_run_without_calling_the_api() {
    let server = TestServer::start();
    let output = run_cli(&server, &["--dry-run", "grok 4.7", "--from", "7d"], &[]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(server.requests().is_empty());

    let body: Value = serde_json::from_str(&stdout(&output)).unwrap();
    assert_eq!(body["model"], "grok-4.7");
    assert_eq!(body["tools"][0]["type"], "x_search");
}

#[test]
fn fails_with_exit_code_2_when_the_key_is_missing() {
    let server = TestServer::start();
    let output = run_cli(&server, &["grok 4.7"], &[("XAI_API_KEY", "")]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("XAI_API_KEY is not set"));
}

#[test]
fn fails_with_exit_code_2_on_usage_errors() {
    let server = TestServer::start();
    let output = run_cli(&server, &["--limit", "0", "grok 4.7"], &[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("--limit"));
}

#[test]
fn fails_with_exit_code_1_on_api_errors() {
    let server = TestServer::start();
    let base = format!("{}/missing", server.base_url());
    let output = run_cli(&server, &["grok 4.7"], &[("XAI_BASE_URL", &base)]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("xAI API error 404"));
}

#[test]
fn surfaces_authentication_hints() {
    let server = TestServer::start();
    let base = format!("{}/unauthorized", server.base_url());
    let output = run_cli(&server, &["grok 4.7"], &[("XAI_BASE_URL", &base)]);
    assert_eq!(output.status.code(), Some(1));
    let text = stderr(&output);
    assert!(text.contains("invalid key"), "stderr: {text}");
    assert!(text.contains("XAI_API_KEY"));
}

#[test]
fn rejects_placeholder_stubs_unless_raw_is_used() {
    let server = TestServer::start();
    let base = format!("{}/stub", server.base_url());

    let normal = run_cli(&server, &["grok 4.7"], &[("XAI_BASE_URL", &base)]);
    assert_eq!(normal.status.code(), Some(1), "stderr: {}", stderr(&normal));
    assert!(stderr(&normal).contains("placeholder result"));

    let raw = run_cli(&server, &["grok 4.7", "--raw"], &[("XAI_BASE_URL", &base)]);
    assert_eq!(raw.status.code(), Some(0), "stderr: {}", stderr(&raw));
    let parsed: Value = serde_json::from_str(&stdout(&raw)).unwrap();
    assert_eq!(parsed["id"], "resp_test_123");
}

#[test]
fn reads_the_query_from_stdin() {
    let server = TestServer::start();
    let output = run_cli_stdin(&server, &["-", "--dry-run"], "query from stdin\n");
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let body: Value = serde_json::from_str(&stdout(&output)).unwrap();
    assert!(
        body["input"][1]["content"]
            .as_str()
            .is_some_and(|content| content.contains("Search X for: query from stdin"))
    );
}

#[test]
fn prints_diagnostics_on_stderr_when_verbose() {
    let server = TestServer::start();
    let output = run_cli(&server, &["grok 4.7", "--verbose"], &[]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let text = stderr(&output);
    assert!(text.contains("xavier: model=grok-4.7 effort=low max_turns=3"));
    assert!(text.contains("xavier: search x_keyword_search \"grok 4.7\" (Latest)"));
    assert!(text.contains("xavier: response=resp_test_123 posts=2 fetched=3"));
}

#[test]
fn prints_help_and_version() {
    let server = TestServer::start();
    let help = run_cli(&server, &["--help"], &[]);
    assert_eq!(help.status.code(), Some(0), "stderr: {}", stderr(&help));
    assert!(stdout(&help).contains("USAGE"));

    let version = run_cli(&server, &["--version"], &[]);
    assert_eq!(
        version.status.code(),
        Some(0),
        "stderr: {}",
        stderr(&version)
    );
    assert_eq!(stdout(&version).trim(), "xavier 0.1.0");
}
