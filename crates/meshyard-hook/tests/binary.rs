#![allow(clippy::unwrap_used, clippy::expect_used)]
use serde_json::{Value, json};
use std::fs::{self, DirBuilder};
use std::io::{ErrorKind, Read, Write};
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
        assert!(path.starts_with("/Volumes/TokensAbuse/"));
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
    // On Darwin the child may already have disconnected; SO_RCVTIMEO then
    // returns EINVAL. Nonblocking reads also work on that disconnected socket.
    stream.set_nonblocking(true).unwrap();
    let started = Instant::now();
    let mut line = Vec::new();
    loop {
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "forwarded EOF deadline"
        );
        let mut buffer = [0; 1025];
        let remaining = 1025 - line.len();
        match stream.read(&mut buffer[..remaining]) {
            Ok(0) => break,
            Ok(n) => {
                line.extend_from_slice(&buffer[..n]);
                assert!(line.len() <= 1024, "envelope exceeds cap");
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("forwarded read failed: {error}"),
        }
    }
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
fn full_socket_queue_fails_silently() {
    let dir = PrivateDir::new();
    let listener =
        UnixListener::bind(dir.socket()).expect("UDS bind: preserve OS error if sandbox denies");
    rustix::net::listen(&listener, 1).unwrap();
    let address = rustix::net::SocketAddrUnix::new(dir.socket()).unwrap();
    let started = Instant::now();
    let mut connections = Vec::new();
    let mut queue_full = false;
    // Retain sequential nonblocking fillers without accepting. Neither the
    // default backlog nor a fixed number of connections proves saturation.
    for _ in 0..64 {
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "queue fill deadline"
        );
        let fd = rustix::net::socket(
            rustix::net::AddressFamily::UNIX,
            rustix::net::SocketType::STREAM,
            None,
        )
        .unwrap();
        let stream = UnixStream::from(fd);
        // SocketFlags::NONBLOCK is unavailable on Apple in pinned rustix 1.1.6.
        stream.set_nonblocking(true).unwrap();
        match rustix::net::connect(&stream, &address) {
            Ok(()) => connections.push(stream),
            Err(error)
                if matches!(
                    error,
                    rustix::io::Errno::CONNREFUSED | rustix::io::Errno::AGAIN
                ) =>
            {
                eprintln!(
                    "observed full queue after {} retained connections: {error}",
                    connections.len()
                );
                queue_full = true;
                break;
            }
            Err(error) => panic!("unexpected nonblocking filler error: {error}"),
        }
    }
    assert!(!connections.is_empty());
    assert!(
        queue_full,
        "queue full must be observed within the filler cap"
    );
    let elapsed = invoke(&dir.socket(), payload(), Duration::ZERO, false);
    eprintln!(
        "silent queue-full failure: {:.3} ms (not a stalled-connect deadline proof)",
        elapsed.as_secs_f64() * 1000.0
    );
    assert!(elapsed < Duration::from_millis(400));
}
