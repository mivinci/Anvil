use serde_json::Value;
#[cfg(not(unix))]
use std::process::{Command, Output};
#[cfg(unix)]
use std::{
  io::{BufRead, Write},
  path::Path,
  process::{Command, Output, Stdio},
};

fn run(root: &std::path::Path, args: &[&str]) -> Output {
  Command::new(env!("CARGO_BIN_EXE_anvil"))
    .arg("--data-dir")
    .arg(root)
    .args(args)
    .output()
    .unwrap()
}
fn json(output: &Output, code: i32) -> Value {
  assert_eq!(
    output.status.code(),
    Some(code),
    "{}",
    String::from_utf8_lossy(&output.stderr)
  );
  assert!(
    output.stderr.is_empty(),
    "{}",
    String::from_utf8_lossy(&output.stderr)
  );
  let value: Value = serde_json::from_slice(&output.stdout).unwrap();
  assert_eq!(value["schema_version"], 1);
  value
}
#[test]
fn empty_list_is_one_clean_json_document() {
  let root = tempfile::tempdir().unwrap();
  let value = json(&run(root.path(), &["java", "list", "--json"]), 0);
  assert_eq!(value["ok"], true);
  assert_eq!(value["data"]["runtimes"], serde_json::json!([]));
  assert!(!root.path().join("runtimes").exists());
}

#[test]
fn empty_client_and_account_lists_are_json_without_creating_state() {
  let root = tempfile::tempdir().unwrap();
  let clients = json(&run(root.path(), &["client", "list", "--json"]), 0);
  assert_eq!(clients["ok"], true);
  assert_eq!(clients["data"]["clients"], serde_json::json!([]));

  let accounts = json(&run(root.path(), &["account", "list", "--json"]), 0);
  assert_eq!(accounts["ok"], true);
  assert_eq!(accounts["data"]["accounts"], serde_json::json!([]));
  assert!(!root.path().join("instances").exists());
  assert!(!root.path().join("accounts.json").exists());
}

#[test]
fn client_and_account_errors_are_json_and_do_not_start_network_work() {
  let root = tempfile::tempdir().unwrap();
  let invalid_name = json(
    &run(
      root.path(),
      &[
        "--json",
        "client",
        "install",
        "../outside",
        "--version",
        "1.21.1",
      ],
    ),
    2,
  );
  assert_eq!(invalid_name["error"]["code"], "invalid_argument");
  assert!(!root.path().join("instances").exists());

  let invalid_client_id = json(
    &run(
      root.path(),
      &["--json", "account", "login", "--client-id", "not-a-guid"],
    ),
    2,
  );
  assert_eq!(invalid_client_id["error"]["code"], "invalid_argument");
  assert!(!root.path().join("accounts.json").exists());

  let no_instance = json(
    &run(
      root.path(),
      &[
        "--json",
        "client",
        "launch",
        "missing",
        "--client-id",
        "00000000-0000-0000-0000-000000000000",
      ],
    ),
    3,
  );
  assert_eq!(no_instance["error"]["code"], "not_found");
  assert!(!root.path().join("instances").exists());
}

#[test]
fn empty_server_list_and_invalid_server_name_have_json_results() {
  let root = tempfile::tempdir().unwrap();
  let listed = json(&run(root.path(), &["server", "list", "--json"]), 0);
  assert_eq!(listed["ok"], true);
  assert_eq!(listed["data"]["servers"], serde_json::json!([]));
  assert!(!root.path().join("instances").exists());

  let invalid = json(
    &run(
      root.path(),
      &[
        "--json",
        "server",
        "create",
        "../outside",
        "--version",
        "1.21.1",
      ],
    ),
    2,
  );
  assert_eq!(invalid["error"]["code"], "invalid_argument");
  assert!(!root.path().join("instances").exists());
}
#[test]
fn argument_errors_and_invalid_java_versions_are_json() {
  let root = tempfile::tempdir().unwrap();
  for args in [
    vec!["--json", "java", "install"],
    vec!["--json", "java", "install", "--version", "nonsense"],
    vec![
      "--json",
      "java",
      "install",
      "--version",
      "21",
      "--vendor",
      "other",
    ],
  ] {
    let value = json(&run(root.path(), &args), 2);
    assert_eq!(value["error"]["code"], "invalid_argument");
  }
}
#[test]
fn uninstall_rejects_traversal_and_reports_missing_runtime() {
  let root = tempfile::tempdir().unwrap();
  assert_eq!(
    json(
      &run(root.path(), &["--json", "java", "uninstall", "../outside"]),
      2
    )["error"]["code"],
    "invalid_argument"
  );
  assert_eq!(
    json(
      &run(
        root.path(),
        &["--json", "java", "uninstall", "temurin-missing"]
      ),
      3
    )["error"]["code"],
    "not_found"
  );
}
#[test]
fn version_and_help_are_available_without_business_work() {
  let root = tempfile::tempdir().unwrap();
  let version = run(root.path(), &["--version"]);
  assert!(version.status.success());
  assert!(String::from_utf8_lossy(&version.stdout).starts_with("anvil "));
  assert!(
    run(root.path(), &["java", "install", "--help"])
      .status
      .success()
  );
  assert!(
    run(root.path(), &["version", "list", "--help"])
      .status
      .success()
  );
}

#[test]
fn another_process_cannot_uninstall_a_locked_runtime() {
  let root = tempfile::tempdir().unwrap();
  let locks = root.path().join("locks");
  std::fs::create_dir(&locks).unwrap();
  let file = std::fs::OpenOptions::new()
    .read(true)
    .write(true)
    .create_new(true)
    .open(locks.join("java-temurin-locked.lock"))
    .unwrap();
  file.try_lock().unwrap();
  assert_eq!(
    json(
      &run(
        root.path(),
        &["--json", "java", "uninstall", "temurin-locked"]
      ),
      3
    )["error"]["code"],
    "busy"
  );
}

#[cfg(unix)]
fn create_server_fixture(root: &Path, server_exit_code: i32) {
  create_server_fixture_with_ready(root, server_exit_code, true);
}

#[cfg(unix)]
fn create_server_fixture_with_ready(root: &Path, server_exit_code: i32, ready: bool) {
  let instances = root.join("instances/server-demo");
  let game = instances.join("game");
  std::fs::create_dir_all(&game).unwrap();
  std::fs::write(instances.join("server.jar"), b"fake jar").unwrap();
  std::fs::write(
    instances.join("instance.json"),
    serde_json::to_vec_pretty(&serde_json::json!({
      "schema_version": 1,
      "type": "server",
      "id": "server-demo",
      "name": "demo",
      "version_id": "1.21.1",
      "java_component": "java-runtime-gamma",
      "java_major": 21,
      "server_jar": {"file":"server.jar", "sha1":"0".repeat(40), "size":8},
      "eula_accepted": false
    }))
    .unwrap(),
  )
  .unwrap();

  let runtime = root.join("runtimes/temurin-fixture");
  let executable = runtime.join("jdk/bin/java");
  std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
  let (os, arch, java_arch) = match (std::env::consts::OS, std::env::consts::ARCH) {
    ("linux", "x86_64") => ("linux", "x64", "amd64"),
    ("linux", "aarch64") => ("linux", "aarch64", "aarch64"),
    ("macos", "x86_64") => ("mac", "x64", "amd64"),
    ("macos", "aarch64") => ("mac", "aarch64", "aarch64"),
    _ => panic!("unsupported fixture platform"),
  };
  let ready_line = if ready {
    "echo '[Server thread/INFO]: Done (0.1s)! For help, type \"help\"'\n"
  } else {
    ""
  };
  let marker = root.join("server-argv.txt");
  let script = format!(
    "#!/bin/sh\nif [ \"$1\" = \"-XshowSettings:properties\" ]; then\n  printf '    java.runtime.version = 21.0.8+9\\n    os.arch = {java_arch}\\n' >&2\n  exit 0\nfi\nprintf '%s|%s|%s\\n' \"$1\" \"$2\" \"$3\" > {}\ntest \"$1\" = \"-jar\" || exit 21\ntest -f \"$2\" || exit 22\ntest \"$3\" = \"nogui\" || exit 23\n{ready_line}while IFS= read -r line; do\n  if [ \"$line\" = \"stop\" ]; then\n    echo 'fake server saved world and stopped'\n    exit {server_exit_code}\n  fi\ndone\n",
    shell_quote(&marker),
  );
  std::fs::write(&executable, script).unwrap();
  use std::os::unix::fs::PermissionsExt;
  std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
  std::fs::write(
    runtime.join("anvil-runtime.json"),
    serde_json::to_vec_pretty(&serde_json::json!({
      "schema_version": 1,
      "id": "temurin-fixture",
      "vendor": "temurin",
      "version": "21.0.8+9",
      "major": 21,
      "image_type": "jre",
      "os": os,
      "arch": arch,
      "archive_sha256": "0".repeat(64),
      "home": "jdk"
    }))
    .unwrap(),
  )
  .unwrap();
}

#[cfg(unix)]
fn shell_quote(path: &Path) -> String {
  format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

#[cfg(unix)]
fn run_server_start(root: &Path) -> Output {
  run_server_start_mode(root, "--json")
}

#[cfg(unix)]
fn run_server_start_mode(root: &Path, mode: &str) -> Output {
  let mut child = Command::new(env!("CARGO_BIN_EXE_anvil"))
    .arg("--data-dir")
    .arg(root)
    .args([mode, "server", "start", "demo", "--accept-eula"])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
  child.stdin.take().unwrap().write_all(b"stop\n").unwrap();
  child.wait_with_output().unwrap()
}

#[cfg(unix)]
fn run_server_start_without_stdin(root: &Path) -> Output {
  Command::new(env!("CARGO_BIN_EXE_anvil"))
    .arg("--data-dir")
    .arg(root)
    .args(["--json", "server", "start", "demo", "--accept-eula"])
    .output()
    .unwrap()
}

#[cfg(unix)]
#[test]
fn server_start_forwards_console_and_logs_without_mixing_json_stdout() {
  let root = tempfile::tempdir().unwrap();
  create_server_fixture(root.path(), 0);
  let output = run_server_start(root.path());
  assert_eq!(output.status.code(), Some(0));
  let value: Value = serde_json::from_slice(&output.stdout).unwrap();
  assert_eq!(value["schema_version"], 1);
  assert_eq!(value["ok"], true);
  assert_eq!(value["data"]["session"]["state"], "exited");
  assert_eq!(value["data"]["session"]["exit_code"], 0);
  assert_eq!(value["data"]["session"]["stop_requested"], true);
  let logs = String::from_utf8_lossy(&output.stderr);
  assert!(logs.contains("fake server saved world and stopped"));
  assert!(String::from_utf8_lossy(&output.stdout).starts_with('{'));
  assert_eq!(
    std::fs::read_to_string(root.path().join("server-argv.txt")).unwrap(),
    format!(
      "-jar|{}|nogui\n",
      root
        .path()
        .join("instances/server-demo/server.jar")
        .display()
    )
  );
  let record: Value = serde_json::from_slice(
    &std::fs::read(root.path().join("instances/server-demo/last-session.json")).unwrap(),
  )
  .unwrap();
  assert_eq!(record["state"], "exited");
  assert!(record["finished_at_unix_ms"].as_u64().is_some());
}

#[cfg(unix)]
#[test]
fn server_start_json_events_report_lifecycle_and_keep_logs_off_stdout() {
  let root = tempfile::tempdir().unwrap();
  create_server_fixture(root.path(), 0);
  let output = run_server_start_mode(root.path(), "--json-events");
  assert_eq!(output.status.code(), Some(0));
  let events: Vec<Value> = String::from_utf8_lossy(&output.stdout)
    .lines()
    .map(|line| serde_json::from_str(line).unwrap())
    .collect();
  assert_eq!(
    events
      .iter()
      .map(|event| event["data"]["state"].as_str().unwrap())
      .collect::<Vec<_>>(),
    ["preparing", "starting", "running", "ready", "exited"]
  );
  assert_eq!(events[0]["schema_version"], 1);
  assert_eq!(events[0]["event"], "state");
  assert!(events[2]["data"]["session_id"].is_string());
  assert_eq!(events[4]["data"]["session"]["exit_code"], 0);
  assert_eq!(events[4]["data"]["session"]["stop_requested"], true);
  assert!(String::from_utf8_lossy(&output.stderr).contains("fake server saved world and stopped"));
}

#[cfg(unix)]
#[test]
fn server_start_json_events_are_available_before_console_exit() {
  let root = tempfile::tempdir().unwrap();
  create_server_fixture(root.path(), 0);
  let mut child = Command::new(env!("CARGO_BIN_EXE_anvil"))
    .arg("--data-dir")
    .arg(root.path())
    .args(["--json-events", "server", "start", "demo", "--accept-eula"])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
  let stdout = child.stdout.take().unwrap();
  let (sender, receiver) = std::sync::mpsc::channel();
  let reader = std::thread::spawn(move || {
    let mut stdout = std::io::BufReader::new(stdout);
    loop {
      let mut line = String::new();
      if stdout.read_line(&mut line).unwrap() == 0 {
        break;
      }
      if sender.send(line).is_err() {
        break;
      }
    }
  });

  for expected in ["preparing", "starting", "running", "ready"] {
    let line = receiver
      .recv_timeout(std::time::Duration::from_secs(10))
      .unwrap_or_else(|error| panic!("missing {expected} event: {error}"));
    let event: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(event["data"]["state"], expected);
  }
  child.stdin.as_mut().unwrap().write_all(b"stop\n").unwrap();
  let exited_line = receiver
    .recv_timeout(std::time::Duration::from_secs(10))
    .expect("missing exited event");
  let exited: Value = serde_json::from_str(&exited_line).unwrap();
  assert_eq!(exited["data"]["state"], "exited");
  assert_eq!(exited["data"]["session"]["exit_code"], 0);

  let output = child.wait_with_output().unwrap();
  assert_eq!(output.status.code(), Some(0));
  reader.join().unwrap();
}

#[cfg(unix)]
#[test]
fn server_start_does_not_wait_for_console_input_when_stdin_is_closed() {
  let root = tempfile::tempdir().unwrap();
  create_server_fixture(root.path(), 0);
  let output = run_server_start_without_stdin(root.path());
  assert_eq!(output.status.code(), Some(0));
  let value: Value = serde_json::from_slice(&output.stdout).unwrap();
  assert_eq!(value["ok"], true);
  assert_eq!(value["data"]["session"]["state"], "exited");
  assert_eq!(value["data"]["session"]["stop_requested"], false);
}

#[cfg(unix)]
#[test]
fn server_start_json_events_report_precondition_failure() {
  let root = tempfile::tempdir().unwrap();
  create_server_fixture(root.path(), 0);
  let output = run(root.path(), &["--json-events", "server", "start", "demo"]);
  assert_eq!(output.status.code(), Some(3));
  assert!(output.stderr.is_empty());
  let events: Vec<Value> = String::from_utf8_lossy(&output.stdout)
    .lines()
    .map(|line| serde_json::from_str(line).unwrap())
    .collect();
  assert_eq!(events.len(), 2);
  assert_eq!(events[0]["data"]["state"], "preparing");
  assert_eq!(events[1]["data"]["state"], "failed");
  assert_eq!(events[1]["data"]["error"]["code"], "eula_required");
}

#[cfg(unix)]
#[test]
fn server_start_json_events_report_child_exit_failure() {
  let root = tempfile::tempdir().unwrap();
  create_server_fixture(root.path(), 7);
  let output = run_server_start_mode(root.path(), "--json-events");
  assert_eq!(output.status.code(), Some(5));
  let events: Vec<Value> = String::from_utf8_lossy(&output.stdout)
    .lines()
    .map(|line| serde_json::from_str(line).unwrap())
    .collect();
  assert_eq!(events[0]["data"]["state"], "preparing");
  assert_eq!(events[3]["data"]["state"], "ready");
  assert_eq!(events[4]["data"]["state"], "failed");
  assert_eq!(events[4]["data"]["error"]["code"], "server_exit");
  assert_eq!(events[4]["data"]["error"]["exit_code"], 5);
  assert_eq!(events[4]["data"]["error"]["process_exit_code"], 7);
}

#[cfg(unix)]
#[test]
fn json_events_are_restricted_to_server_start() {
  let root = tempfile::tempdir().unwrap();
  let output = run(root.path(), &["--json-events", "server", "list"]);
  assert_eq!(output.status.code(), Some(2));
  assert!(output.stderr.is_empty());
  let events: Vec<Value> = String::from_utf8_lossy(&output.stdout)
    .lines()
    .map(|line| serde_json::from_str(line).unwrap())
    .collect();
  assert_eq!(events.len(), 1);
  assert_eq!(events[0]["data"]["state"], "failed");
  assert_eq!(events[0]["data"]["error"]["code"], "invalid_argument");
}

#[cfg(unix)]
#[test]
fn json_and_json_events_cannot_be_combined() {
  let root = tempfile::tempdir().unwrap();
  let output = run(root.path(), &["--json-events", "--json", "server", "list"]);
  assert_eq!(output.status.code(), Some(2));
  assert!(output.stderr.is_empty());
  let events: Vec<Value> = String::from_utf8_lossy(&output.stdout)
    .lines()
    .map(|line| serde_json::from_str(line).unwrap())
    .collect();
  assert_eq!(events.len(), 1);
  assert_eq!(events[0]["data"]["state"], "failed");
  assert_eq!(events[0]["data"]["error"]["code"], "invalid_argument");
}

#[cfg(unix)]
#[test]
fn server_start_cannot_report_success_if_exit_precedes_ready() {
  let root = tempfile::tempdir().unwrap();
  create_server_fixture_with_ready(root.path(), 0, false);
  let output = run_server_start(root.path());
  assert_eq!(output.status.code(), Some(5));
  let value: Value = serde_json::from_slice(&output.stdout).unwrap();
  assert_eq!(value["ok"], false);
  assert_eq!(value["error"]["code"], "server_not_ready");
  assert!(
    value["error"]["message"]
      .as_str()
      .unwrap()
      .contains("before reaching Ready")
  );
}

#[cfg(unix)]
#[test]
fn server_start_json_events_fail_if_ready_was_never_observed() {
  let root = tempfile::tempdir().unwrap();
  create_server_fixture_with_ready(root.path(), 0, false);
  let output = run_server_start_mode(root.path(), "--json-events");
  assert_eq!(output.status.code(), Some(5));
  let events: Vec<Value> = String::from_utf8_lossy(&output.stdout)
    .lines()
    .map(|line| serde_json::from_str(line).unwrap())
    .collect();
  let states: Vec<_> = events
    .iter()
    .map(|event| event["data"]["state"].as_str().unwrap())
    .collect();
  assert_eq!(states, ["preparing", "starting", "running", "failed"]);
  assert_eq!(events[3]["data"]["error"]["code"], "server_not_ready");
  assert_eq!(events[3]["data"]["error"]["process_exit_code"], 0);
}

#[cfg(unix)]
#[test]
fn server_start_returns_a_machine_readable_nonzero_exit() {
  let root = tempfile::tempdir().unwrap();
  create_server_fixture(root.path(), 7);
  let output = run_server_start(root.path());
  assert_eq!(output.status.code(), Some(5));
  let value: Value = serde_json::from_slice(&output.stdout).unwrap();
  assert_eq!(value["ok"], false);
  assert_eq!(value["error"]["code"], "server_exit");
  assert_eq!(value["error"]["exit_code"], 5);
  assert!(String::from_utf8_lossy(&output.stderr).contains("fake server saved world and stopped"));
  let record: Value = serde_json::from_slice(
    &std::fs::read(root.path().join("instances/server-demo/last-session.json")).unwrap(),
  )
  .unwrap();
  assert_eq!(record["state"], "exited");
  assert_eq!(record["exit_code"], 7);
}

#[cfg(unix)]
#[test]
fn server_start_requires_eula_as_a_json_precondition() {
  let root = tempfile::tempdir().unwrap();
  create_server_fixture(root.path(), 0);
  let output = run(root.path(), &["--json", "server", "start", "demo"]);
  let value = json(&output, 3);
  assert_eq!(value["error"]["code"], "eula_required");
  assert!(
    !root
      .path()
      .join("instances/server-demo/game/eula.txt")
      .exists()
  );
  assert!(
    !root
      .path()
      .join("instances/server-demo/last-session.json")
      .exists()
  );
}
