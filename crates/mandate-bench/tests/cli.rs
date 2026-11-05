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
        // The supplied key template leaves optional URL overrides blank.
        fs::write(
            path.join(".env"),
            "OPENAI_BASE_URL=\nDEEPSEEK_BASE_URL=\nNVIDIA_BASE_URL=\n",
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
            .env_remove("META_BASE_URL")
            .env_remove("MODEL_API_KEY")
            .env_remove("MB_TEST_API_KEY");
        if key_present {
            process
                .env("MB_TEST_API_KEY", KEY)
                .env("MODEL_API_KEY", KEY);
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

fn muse_run_root(signoff: &str, status: &str) -> TempRoot {
    let root = TempRoot::new("https://api.meta.ai/v1");
    fs::write(
        root.0.join("config.toml"),
        include_str!("../../../configs/muse-diagnostic.toml"),
    )
    .unwrap();
    fs::create_dir_all(root.0.join("docs")).unwrap();
    fs::write(root.0.join("docs/SIGNOFF.md"), signoff).unwrap();
    fs::write(root.0.join("docs/STATUS.md"), status).unwrap();
    fs::create_dir_all(root.0.join("runs")).unwrap();
    fs::write(
        root.0.join("runs/muse-tool-eligibility.json"),
        serde_json::to_vec(&json!({
            "muse-spark-1.1": {
                "status": "tool_call_supported",
                "source": "provider_api",
                "base_url": "https://api.meta.ai/v1",
                "reasoning_effort": "medium"
            }
        }))
        .unwrap(),
    )
    .unwrap();
    root
}

const MUSE_TASK_IDS: [&str; 9] = [
    "t00_control",
    "t01_hidden_fees",
    "t02_expired_offer",
    "t03_compatibility",
    "t04_seller_injection",
    "t05_bundle_vs_unit",
    "t06_ship_vs_delivery",
    "t07_final_sale",
    "t08_no_valid_offer",
];

fn muse_status_rows(task_ids: &[&str], unchecked: Option<&str>) -> String {
    let mut status = String::from(
        "  | Task | Built | Oracle 1.0 | Mutants fail | ozgur sign-off |  \n  | --- | --- | --- | --- | --- |  \n",
    );
    for task_id in task_ids {
        let mark = if Some(*task_id) == unchecked {
            "☐"
        } else {
            "✅"
        };
        status.push_str(&format!("  | {task_id} | ☑ | ☑ | ☑ |  {mark}  |  \n"));
    }
    status
}

const CHECKED_MUSE_SIGNOFF: &str =
    "  - [x]   Task and reference sign-off  \n  - [x] Grader and metrics sign-off \n";

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
fn muse_smoke_replays_the_call_and_persists_both_responses() {
    let mut first = response(
        json!([
            {"type":"reasoning","id":"rs_smoke","summary":[],"encrypted_content":"opaque-fixture"},
            {"type":"function_call","id":"fc_smoke","call_id":"call_smoke","name":"smoke_echo","arguments":"{\"value\":\"ready\"}"}
        ]),
        "completed",
    );
    first["model"] = json!("muse-spark-1.1");
    let mut second = response(
        json!([
            {"type":"message","role":"assistant","content":[{"type":"output_text","text":"smoke-roundtrip-confirmed"}]}
        ]),
        "completed",
    );
    second["model"] = json!("muse-spark-1.1");
    let (url, handle, requests) = server(vec![(200, first), (200, second)]);
    let root = TempRoot::new(&url);
    let config = include_str!("../../../configs/muse-smoke.toml")
        .replace("https://api.meta.ai/v1", &url)
        .replace("rpm = 6\n", "rpm = 6000\n");
    fs::write(root.0.join("config.toml"), config).unwrap();
    let output = root.command("smoke", true);
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    handle.join().unwrap();

    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0]["tool_choice"], "auto");
    assert_eq!(requests[1]["reasoning"]["effort"], "medium");
    assert_eq!(requests[1]["tool_choice"], "auto");
    let input = requests[1]["input"].as_array().unwrap();
    assert!(
        input.iter().any(
            |item| item["type"] == "reasoning" && item["encrypted_content"] == "opaque-fixture"
        )
    );
    assert!(input.iter().any(|item| {
        item["type"] == "function_call_output"
            && item["call_id"] == "call_smoke"
            && item["output"]
                .as_str()
                .unwrap()
                .contains("smoke-roundtrip-confirmed")
    }));
    assert!(
        input.last().unwrap()["content"]
            .as_str()
            .unwrap()
            .contains("Do not make any further tool calls.")
    );
    let (meta, scores, directory) = root.artifacts();
    assert_eq!(meta["http_attempts"], 2);
    assert_eq!(meta["token_usage"]["reasoning"], 4);
    assert_eq!(scores[0]["status"], "tool_call_supported");
    assert_eq!(
        read_json(&directory.join("outputs/model-0.json"))
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(!root.0.join("runs/muse-tool-eligibility.json").exists());
}

#[test]
fn muse_smoke_preserves_sanitized_validation_details_and_stops() {
    let (url, handle, requests) = server(vec![(
        400,
        json!({
            "error": {
                "message": format!("Unsupported tool choice. Request key was {KEY}"),
                "param": "tool_choice",
                "type": "invalid_request_error",
                "code": null,
            },
            "debug": "discard this field"
        }),
    )]);
    let root = TempRoot::new(&url);
    let config =
        include_str!("../../../configs/muse-smoke.toml").replace("https://api.meta.ai/v1", &url);
    fs::write(root.0.join("config.toml"), config).unwrap();
    assert!(!root.command("smoke", true).status.success());
    handle.join().unwrap();
    assert_eq!(requests.lock().unwrap().len(), 1);

    let (meta, scores, directory) = root.artifacts();
    assert_eq!(meta["http_attempts"], 1);
    assert_eq!(scores[0]["status"], "api_error");
    let transcript = read_json(&directory.join("transcripts/model-0.json"));
    let detail = &transcript["api_error"]["raw"];
    assert_eq!(detail["status"], 400);
    assert_eq!(detail["error"]["param"], "tool_choice");
    assert_eq!(detail["error"]["type"], "invalid_request_error");
    assert_eq!(
        detail["error"]["message"],
        "Unsupported tool choice. Request key was [REDACTED]"
    );
    assert!(detail.get("debug").is_none());
    assert!(!transcript.to_string().contains(KEY));
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
    assert_eq!(scores[0]["metrics_policy_version"], 2);
    assert_eq!(scores[0]["outcome_pass"], true);
    assert_eq!(scores[0]["strict_pass"], true);
    assert_eq!(scores[0]["format_compliant"], true);
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
    assert!(scores[0]["outcome_pass"].is_null());
    assert!(scores[0]["strict_pass"].is_null());
    assert!(scores[0]["format_compliant"].is_null());
    assert!(meta["cost_estimate_usd"].is_null());
    assert!(!meta.to_string().contains(KEY));
}

#[test]
fn provider_api_error_stops_queued_trials_without_fabricating_attempts() {
    let (url, handle, requests) =
        server(vec![(429, json!({"error":{"code":"rate_limit_exceeded"}}))]);
    let root = TempRoot::new(&url);
    let config = fs::read_to_string(root.0.join("config.toml"))
        .unwrap()
        .replacen("trials = 1", "trials = 2", 1);
    fs::write(root.0.join("config.toml"), config).unwrap();
    assert!(!root.command("run", true).status.success());
    handle.join().unwrap();
    assert_eq!(requests.lock().unwrap().len(), 1);
    let (meta, scores, _) = root.artifacts();
    assert_eq!(scores.len(), 1);
    assert_eq!(meta["state"], "incomplete");
    assert_eq!(meta["unstarted_trials"], 1);
    assert_eq!(meta["halted_providers"], json!(["openai"]));
}

#[test]
fn error_after_a_successful_request_retains_known_cost_and_safe_error_codes() {
    let (url, handle, _) = server(vec![
        (200, order_reply()),
        (
            429,
            json!({"error":{"code":"rate_limit_exceeded","type":"requests","message":format!("sensitive {KEY}")}}),
        ),
    ]);
    let root = TempRoot::new(&url);
    assert!(!root.command("run", true).status.success());
    handle.join().unwrap();
    let (meta, scores, directory) = root.artifacts();
    assert_eq!(scores[0]["status"], "api_error");
    assert!(meta["cost_estimate_usd"].is_null());
    assert_eq!(
        meta["known_cost_estimate_usd"]
            .as_number()
            .unwrap()
            .to_string()
            .parse::<Decimal>()
            .unwrap(),
        Decimal::new(173, 7)
    );
    let entry = fs::read_dir(directory.join("transcripts"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap();
    let transcript = read_json(&entry.path());
    assert_eq!(transcript["api_error"]["error_code"], "rate_limit_exceeded");
    assert_eq!(transcript["api_error"]["error_type"], "requests");
    assert!(!transcript.to_string().contains(KEY));
    assert!(transcript["api_error"]["raw"].is_null());
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

#[test]
fn prospective_ask_user_metrics_preserve_the_raw_format_failure() {
    let ask = response(
        json!([{"type":"function_call","id":"item-ask","call_id":"call-ask","name":"ask_user","arguments":"{\"question\":\"No offer qualifies; may the mandate change?\"}"}]),
        "completed",
    );
    let answer =
        json!({"decision":"ASK_USER","order":null,"reason":"No eligible offer"}).to_string();
    let final_answer = response(
        json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":answer}]}]),
        "completed",
    );
    let (url, handle, _) = server(vec![(200, ask), (200, final_answer)]);
    let root = TempRoot::new(&url);
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    fs::create_dir_all(root.0.join("tasks/t08_no_valid_offer")).unwrap();
    for file in [
        "tasks/t08_no_valid_offer/task.json",
        "reference/t08_no_valid_offer.json",
    ] {
        fs::copy(repo.join(file), root.0.join(file)).unwrap();
    }
    let config = fs::read_to_string(root.0.join("config.toml"))
        .unwrap()
        .replace("t00_control", "t08_no_valid_offer");
    fs::write(root.0.join("config.toml"), config).unwrap();
    assert!(root.command("run", true).status.success());
    handle.join().unwrap();
    let (meta, scores, directory) = root.artifacts();
    assert_eq!(meta["metrics_policy_version"], 2);
    assert_eq!(scores[0]["status"], "pass");
    assert_eq!(scores[0]["outcome_pass"], true);
    assert_eq!(scores[0]["strict_pass"], false);
    assert_eq!(scores[0]["format_compliant"], false);
    assert_eq!(scores[0]["grade"]["pass"], false);
    assert!(scores[0]["failure_label"].is_null());
    let transcript = read_json(
        &directory
            .join("transcripts")
            .join("model-0-t08_no_valid_offer-trial-1.json"),
    );
    assert!(
        !transcript["final_output"]
            .as_str()
            .unwrap()
            .contains("currency")
    );
}

#[test]
fn real_paid_config_requires_the_cumulative_guard_before_key_preflight() {
    let root = TempRoot::new("https://api.openai.com/v1");
    let config = fs::read_to_string(root.0.join("config.toml"))
        .unwrap()
        .replace("mock-model", "gpt-6-luna");
    fs::write(root.0.join("config.toml"), config).unwrap();
    let output = root.command("smoke", true);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cumulative budget guard"));
    assert!(!root.0.join("runs").exists());
}

#[test]
fn muse_run_accepts_whitespace_and_checked_marks_before_loading_tasks() {
    let status = muse_status_rows(&MUSE_TASK_IDS, None);
    let root = muse_run_root(CHECKED_MUSE_SIGNOFF, &status);

    let output = root.command("run", true);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(
        stderr.contains("tasks/t01_hidden_fees/task.json"),
        "{stderr}"
    );
    assert!(!stderr.contains("owner sign-off"), "{stderr}");
    assert_eq!(fs::read_dir(root.0.join("runs")).unwrap().count(), 1);
}

#[test]
fn muse_run_rejects_missing_duplicate_or_unchecked_owner_signoffs_before_dispatch() {
    let mut missing_task = MUSE_TASK_IDS.to_vec();
    missing_task.pop();
    let mut duplicate_task = MUSE_TASK_IDS.to_vec();
    duplicate_task.push("t03_compatibility");
    let cases = [
        (
            "unchecked SIGNOFF box",
            "  - [ ] Task and reference sign-off\n  - [x] Grader and metrics sign-off\n".to_owned(),
            muse_status_rows(&MUSE_TASK_IDS, None),
            "Task and reference sign-off",
        ),
        (
            "missing STATUS row",
            CHECKED_MUSE_SIGNOFF.to_owned(),
            muse_status_rows(&missing_task, None),
            "t08_no_valid_offer",
        ),
        (
            "duplicate STATUS row",
            CHECKED_MUSE_SIGNOFF.to_owned(),
            muse_status_rows(&duplicate_task, None),
            "t03_compatibility",
        ),
        (
            "unchecked STATUS cell",
            CHECKED_MUSE_SIGNOFF.to_owned(),
            muse_status_rows(&MUSE_TASK_IDS, Some("t06_ship_vs_delivery")),
            "t06_ship_vs_delivery",
        ),
    ];

    for (case, signoff, status, expected) in cases {
        let root = muse_run_root(&signoff, &status);
        let output = root.command("run", true);
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert!(!output.status.success(), "{case} unexpectedly passed");
        assert!(stderr.contains(expected), "{case}: {stderr}");
        assert_eq!(fs::read_dir(root.0.join("runs")).unwrap().count(), 1);
    }
}
