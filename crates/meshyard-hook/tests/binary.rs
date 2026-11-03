#![allow(clippy::unwrap_used, clippy::expect_used)]
use serde_json::{Value, json};
use std::fs::{self, DirBuilder};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

struct PrivateDir(PathBuf);
impl PrivateDir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let base = std::env::var_os("TMPDIR").expect("USB TMPDIR required");
        let path = PathBuf::from(base).join(format!(
            "h{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        assert!(path.starts_with("/Volumes/MeshyardProjects/"));
        DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self(path)
    }
    fn socket(&self) -> PathBuf {
        self.0.join("s")
    }
}
impl Drop for PrivateDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn invoke(path: &Path, input: Vec<u8>, delay: Duration, hold_open: bool) -> Duration {
    let start = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_meshyard-hook"))
        .args(["claude-code", "--socket"])
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let writer = thread::spawn(move || {
        thread::sleep(delay);
        let _ = stdin.write_all(&input);
        if hold_open {
            thread::sleep(Duration::from_millis(250));
        }
    });
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if start.elapsed() > Duration::from_secs(2) {
            let _ = child.kill();
            panic!("hook exceeded test watchdog");
        }
        thread::sleep(Duration::from_millis(1));
    }
    let output = child.wait_with_output().unwrap();
    let elapsed = start.elapsed();
    assert!(output.status.success());
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
    eprintln!("hook raw timing: {:.3} ms", elapsed.as_secs_f64() * 1000.0);
    assert!(
        elapsed < Duration::from_millis(750),
        "generous process-launch/CI margin"
    );
    writer.join().unwrap();
    elapsed
}
fn payload() -> Vec<u8> {
    serde_json::to_vec(&json!({"hook_event_name":"PreToolUse","session_id":"s","tool_use_id":"t","tool_name":"Bash","tool_input":{"command":"cargo test COMMAND_SENTINEL", "file_path":"PATH_SENTINEL"},"prompt":"PROMPT_SENTINEL","cwd":"CWD_SENTINEL","tool_response":"OUTPUT_SENTINEL"})).unwrap()
}

#[test]
fn empty_malformed_oversize_absent_and_stalled_stdin_are_silent() {
    let dir = PrivateDir::new();
    for input in [
        vec![],
        b"{".to_vec(),
        vec![b'x'; 1024 * 1024 + 1],
        br#"{"hook_event_name":"Stop"}"#.to_vec(),
        br#"{"hook_event_name":"Stop","session_id":"bad/id"}"#.to_vec(),
        payload(),
    ] {
        invoke(&dir.socket(), input, Duration::ZERO, false);
    }
    let elapsed = invoke(&dir.socket(), vec![b'{'], Duration::ZERO, true);
    assert!(elapsed >= Duration::from_millis(80));
    invoke(Path::new("relative.sock"), payload(), Duration::ZERO, false);
}

#[test]
fn actual_forwarded_json_is_private_and_one_line() {
    let dir = PrivateDir::new();
    let listener =
        UnixListener::bind(dir.socket()).expect("UDS bind: preserve OS error if sandbox denies");
    listener.set_nonblocking(true).unwrap();
    invoke(&dir.socket(), payload(), Duration::ZERO, false);
    let (mut stream, _) = listener.accept().unwrap();
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    let mut line = Vec::new();
    stream.read_to_end(&mut line).unwrap();
    assert!(line.len() <= 1024 && line.ends_with(b"\n"));
    assert_eq!(line.iter().filter(|c| **c == b'\n').count(), 1);
    let text = String::from_utf8(line.clone()).unwrap();
    assert!(!text.contains("SENTINEL"));
    let value: Value = serde_json::from_slice(&line).unwrap();
    assert_eq!(
        value,
        json!({"v":1,"agent":"claude_code","event":"PreToolUse","session_id":"s","tool_use_id":"t","tool":"shell","command_class":"test"})
    );
}

#[test]
fn rejected_inputs_and_socket_metadata_never_connect() {
    let dir = PrivateDir::new();
    let listener =
        UnixListener::bind(dir.socket()).expect("UDS bind: preserve OS error if sandbox denies");
    listener.set_nonblocking(true).unwrap();
    for input in [
        b"{".to_vec(),
        vec![b'x'; 1024 * 1024 + 1],
        br#"{"hook_event_name":"Stop"}"#.to_vec(),
        br#"{"hook_event_name":"Stop","session_id":"bad/id"}"#.to_vec(),
        br#"{"hook_event_name":"Notification","session_id":"s"}"#.to_vec(),
    ] {
        invoke(&dir.socket(), input, Duration::ZERO, false);
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
    let alias = dir.0.join("alias");
    symlink(dir.socket(), &alias).unwrap();
    invoke(&alias, payload(), Duration::ZERO, false);
    assert!(listener.accept().is_err());
    let parent_alias = dir.0.with_extension("link");
    symlink(&dir.0, &parent_alias).unwrap();
    invoke(&parent_alias.join("s"), payload(), Duration::ZERO, false);
    fs::remove_file(parent_alias).unwrap();
    assert!(listener.accept().is_err());
    fs::set_permissions(&dir.0, fs::Permissions::from_mode(0o750)).unwrap();
    invoke(&dir.socket(), payload(), Duration::ZERO, false);
    assert!(listener.accept().is_err());
    fs::set_permissions(&dir.0, fs::Permissions::from_mode(0o700)).unwrap();
    let regular = dir.0.join("file");
    fs::write(&regular, b"").unwrap();
    invoke(&regular, payload(), Duration::ZERO, false);
    assert!(listener.accept().is_err());
}

#[test]
fn saturated_socket_and_delayed_stdin_share_the_deadline() {
    let dir = PrivateDir::new();
    let listener =
        UnixListener::bind(dir.socket()).expect("UDS bind: preserve OS error if sandbox denies");
    let (tx, rx) = std::sync::mpsc::channel();
    let mut workers = Vec::new();
    for _ in 0..32 {
        let path = dir.socket();
        let tx = tx.clone();
        workers.push(thread::spawn(move || {
            let _ = tx.send(UnixStream::connect(path));
        }));
    }
    drop(tx);
    let mut connections = Vec::new();
    while let Ok(result) = rx.recv_timeout(Duration::from_millis(30)) {
        if let Ok(stream) = result {
            connections.push(stream);
        }
    }
    assert!(!connections.is_empty());
    assert!(
        connections.len() < 32,
        "backlog must be saturated; otherwise test is not evidence"
    );
    let elapsed = invoke(&dir.socket(), payload(), Duration::from_millis(75), false);
    eprintln!(
        "shared deadline delayed stdin + saturated connect: {:.3} ms",
        elapsed.as_secs_f64() * 1000.0
    );
    assert!(elapsed < Duration::from_millis(400));
    drop(listener);
    drop(connections);
    drop(rx);
    for worker in workers {
        worker.join().unwrap();
    }
}
