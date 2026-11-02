use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use rust_decimal::Decimal;
use serde_json::{Value, json};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const KEY: &str = "local-only-test-credential";

struct TempRoot(PathBuf);
impl TempRoot {
    fn new(base_url: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "mandate-m2-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::create_dir_all(path.join("tasks/t00_control")).unwrap();
        fs::create_dir(path.join("reference")).unwrap();
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for file in ["tasks/t00_control/task.json", "reference/t00_control.json"] {
            fs::copy(repo.join(file), path.join(file)).unwrap();
        }
        fs::write(
            path.join("config.toml"),
            format!(
                r#"
trials = 1
max_turns = 4
tasks = ["t00_control"]
[providers.openai]
base_url = "{base_url}"
key_env = "MB_TEST_API_KEY"
api = "responses"
max_concurrency = 1
max_retries = 0
timeout_seconds = 5
[[models]]
id = "mock-model"
provider = "openai"
max_output_tokens = 1024
[models.price]
input_per_million = 1.0
cached_input_per_million = 0.1
output_per_million = 2.0
"#
            ),
        )
        .unwrap();
        Self(path)
    }

    fn command(&self, command: &str, key_present: bool) -> Output {
        let mut process = Command::new(env!("CARGO_BIN_EXE_mandate-bench"));
        process
            .args([
                "--root",
                self.0.to_str().unwrap(),
                command,
                "--config",
                "config.toml",
            ])
            .env_remove("OPENAI_BASE_URL")
            .env_remove("MB_TEST_API_KEY");
        if key_present {
            process.env("MB_TEST_API_KEY", KEY);
        }
        process.output().unwrap()
    }

    fn artifacts(&self) -> (Value, Vec<Value>, PathBuf) {
        let entries = fs::read_dir(self.0.join("runs"))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(entries.len(), 1);
        let directory = entries[0].path();
        let meta = read_json(&directory.join("run_meta.json"));
        let scores = fs::read_to_string(directory.join("scores.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        (meta, scores, directory)
    }
}
impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn server(replies: Vec<(u16, Value)>) -> (String, thread::JoinHandle<()>, Arc<Mutex<Vec<Value>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();
    let handle = thread::spawn(move || {
        for (status, reply) in replies {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(_) => return,
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let header_end = loop {
                let mut buffer = [0; 4096];
                let count = stream.read(&mut buffer).unwrap();
                if count == 0 {
                    return;
                }
                bytes.extend_from_slice(&buffer[..count]);
                if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                    break end + 4;
                }
            };
            let header = String::from_utf8_lossy(&bytes[..header_end]);
            assert!(header.starts_with("POST /v1/responses "));
            let length: usize = header
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse().unwrap())
                })
                .unwrap();
            while bytes.len() < header_end + length {
                let mut buffer = [0; 4096];
                let count = stream.read(&mut buffer).unwrap();
                if count == 0 {
                    return;
                }
                bytes.extend_from_slice(&buffer[..count]);
            }
            captured
                .lock()
                .unwrap()
                .push(serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap());
            let body = serde_json::to_vec(&reply).unwrap();
            write!(stream, "HTTP/1.1 {status} Mock\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
            stream.write_all(&body).unwrap();
        }
    });
    (format!("http://{address}/v1"), handle, requests)
}

fn response(output: Value, status: &str) -> Value {
    json!({"id":"response-test","status":status,"model":"returned-model-version","output":output,
        "usage":{"input_tokens":10,"input_tokens_details":{"cached_tokens":3},"output_tokens":5,"output_tokens_details":{"reasoning_tokens":2}}})
}

fn order_reply() -> Value {
    response(
        json!([{"type":"function_call","id":"item-1","call_id":"call-1","name":"place_order","arguments":"{\"offer_id\":\"B\",\"packs\":1,\"authorized_total\":59.50}"}]),
        "completed",
    )
}

fn final_reply(status: &str, malformed: bool) -> Value {
    let text = if malformed {
        "not JSON".into()
    } else {
        json!({"decision":"BUY","order":{"offer_id":"B","packs":1},"total":59.50,"currency":"USD","reason":format!("Done {KEY}")}).to_string()
    };
    response(
        json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":text}]}]),
        status,
    )
}

#[test]
fn cli_runs_a_fresh_trial_and_persists_real_returned_model_and_totals() {
    let (url, handle, requests) = server(vec![
        (200, order_reply()),
        (200, final_reply("completed", false)),
    ]);
    let root = TempRoot::new(&url);
    let output = root.command("run", true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    handle.join().unwrap();
    let (meta, scores, directory) = root.artifacts();
    assert_eq!(scores[0]["status"], "pass");
    assert_eq!(
        scores[0]["returned_models"],
        json!(["returned-model-version"])
    );
    assert_eq!(meta["state"], "completed");
    assert_eq!(meta["source"], "local_mock");
    assert_eq!(meta["token_usage"]["input"], 20);
    assert_eq!(meta["token_usage"]["output"], 10);
    assert_eq!(meta["token_usage"]["reasoning"], 4);
    assert_eq!(meta["api_error_count"], 0);
    assert_eq!(
        meta["cost_estimate_usd"]
            .as_number()
            .unwrap()
            .to_string()
            .parse::<Decimal>()
            .unwrap(),
        Decimal::new(346, 7)
    );
    let input = &requests.lock().unwrap()[0]["input"];
    assert!(!input.to_string().contains("Hidden oracle canary"));
    for folder in ["outputs", "transcripts"] {
        for entry in fs::read_dir(directory.join(folder)).unwrap() {
            assert!(
                !fs::read_to_string(entry.unwrap().path())
                    .unwrap()
                    .contains(KEY)
            );
        }
    }
}

#[test]
fn incomplete_model_output_is_a_failed_trial_not_an_api_error() {
    let (url, handle, _) = server(vec![
        (200, order_reply()),
        (200, final_reply("incomplete", false)),
    ]);
    let root = TempRoot::new(&url);
    assert!(root.command("run", true).status.success());
    handle.join().unwrap();
    let (meta, scores, _) = root.artifacts();
    assert_eq!(scores[0]["status"], "fail");
    assert_eq!(scores[0]["failure_label"], "no_decision");
    assert_eq!(meta["api_error_count"], 0);
}

#[test]
fn ordinary_invalid_final_text_is_a_schema_failure() {
    let (url, handle, _) = server(vec![
        (200, order_reply()),
        (200, final_reply("completed", true)),
    ]);
    let root = TempRoot::new(&url);
    assert!(root.command("run", true).status.success());
    handle.join().unwrap();
    let (_, scores, _) = root.artifacts();
    assert_eq!(scores[0]["failure_label"], "schema_error");
}

#[test]
fn api_errors_are_saved_once_without_grades_or_secret_error_bodies() {
    let (url, handle, requests) = server(vec![(401, json!({"error":format!("denied {KEY}")}))]);
    let root = TempRoot::new(&url);
    assert!(!root.command("run", true).status.success());
    handle.join().unwrap();
    assert_eq!(requests.lock().unwrap().len(), 1);
    let (meta, scores, _) = root.artifacts();
    assert_eq!(meta["api_error_count"], 1);
    assert_eq!(meta["state"], "completed_with_api_errors");
    assert_eq!(scores[0]["status"], "api_error");
    assert!(scores[0]["grade"].is_null());
    assert!(meta["cost_estimate_usd"].is_null());
    assert!(!meta.to_string().contains(KEY));
}

#[test]
fn smoke_sends_one_tool_probe_and_marks_it_as_non_benchmark_data() {
    let reply = response(
        json!([{"type":"function_call","id":"item-1","call_id":"call-1","name":"smoke_echo","arguments":"{\"value\":\"ready\"}"}]),
        "completed",
    );
    let (url, handle, requests) = server(vec![(200, reply)]);
    let root = TempRoot::new(&url);
    assert!(root.command("smoke", true).status.success());
    handle.join().unwrap();
    let (meta, scores, _) = root.artifacts();
    assert_eq!(requests.lock().unwrap().len(), 1);
    assert_eq!(meta["kind"], "smoke");
    assert_eq!(scores[0]["status"], "tool_call_supported");
    assert!(scores[0].get("grade").is_none());
}

#[test]
fn missing_keys_fail_preflight_before_any_run_directory_is_created() {
    let root = TempRoot::new("http://127.0.0.1:1/v1");
    let output = root.command("smoke", false);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("MB_TEST_API_KEY"));
    assert!(!root.0.join("runs").exists());
}
