//! Private, allowlisted local hook transport. No raw payload is retained after normalization.
use serde::Serialize;
use serde_json::Value;
use std::fs::{self, Metadata};
use std::io::{Read, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

pub const INPUT_LIMIT: usize = 1024 * 1024;
pub const ENVELOPE_LIMIT: usize = 1024;
pub const TOTAL_BUDGET: Duration = Duration::from_millis(150);
const READ_BUDGET: Duration = Duration::from_millis(100);
const IO_BUDGET: Duration = Duration::from_millis(20);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandClass {
    Test,
    GitMerge,
    Other,
}

/// Bounded heuristic, not a shell interpreter. Quoted separators remain in their token.
pub fn classify(command: &str) -> CommandClass {
    let mut end = command.len().min(4096);
    while !command.is_char_boundary(end) {
        end -= 1;
    }
    let mut segments = vec![Vec::<String>::new()];
    let mut token = String::new();
    let mut quote = None;
    let mut escaped = false;
    let mut chars = command[..end].chars().peekable();
    while let Some(ch) = chars.next() {
        if escaped {
            token.push(ch);
            escaped = false;
        } else if ch == '\\' && quote != Some('\'') {
            escaped = true;
        } else if quote == Some(ch) {
            quote = None;
        } else if quote.is_none() && matches!(ch, '\'' | '"') {
            quote = Some(ch);
        } else if quote.is_none()
            && (ch.is_whitespace()
                || matches!(ch, '|' | ';')
                || (ch == '&' && chars.peek() == Some(&'&')))
        {
            if let Some(segment) = segments.last_mut()
                && !token.is_empty()
            {
                segment.push(std::mem::take(&mut token));
            }
            if matches!(ch, '&' | '|' | ';') {
                if ch == '&' {
                    chars.next();
                }
                segments.push(Vec::new());
            }
        } else {
            token.push(ch);
        }
    }
    if let Some(segment) = segments.last_mut()
        && !token.is_empty()
    {
        segment.push(token);
    }
    let mut result = CommandClass::Other;
    for segment in segments {
        let words: Vec<&str> = segment.iter().map(String::as_str).collect();
        let mut words = words.as_slice();
        while let Some(first) = words.first() {
            if *first == "env" || assignment(first) {
                words = &words[1..];
            } else {
                break;
            }
        }
        let class = match words {
            ["git", "merge" | "rebase" | "cherry-pick", ..] => CommandClass::GitMerge,
            ["git", "pull", rest @ ..] if rest.contains(&"--rebase") => CommandClass::GitMerge,
            ["cargo", "test" | "nextest", ..]
            | ["npm" | "pnpm" | "yarn" | "bun", "test", ..]
            | ["npm" | "pnpm" | "yarn" | "bun", "run", "test", ..]
            | ["npx", "vitest" | "jest", ..]
            | ["vitest" | "jest" | "pytest", ..]
            | ["go" | "swift" | "xcodebuild" | "make", "test", ..] => CommandClass::Test,
            _ => CommandClass::Other,
        };
        if class == CommandClass::GitMerge {
            return class;
        }
        if class == CommandClass::Test {
            result = class;
        }
    }
    result
}

fn assignment(token: &str) -> bool {
    token.split_once('=').is_some_and(|(key, _)| {
        !key.is_empty()
            && key
                .bytes()
                .enumerate()
                .all(|(i, c)| c == b'_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit()))
    })
}

fn valid_id(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._:-".contains(&c))
}

#[derive(Serialize)]
struct Envelope<'a> {
    v: u8,
    agent: &'a str,
    event: &'a str,
    session_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    turn_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_use_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    command_class: Option<CommandClass>,
}

/// Produces the entire newline envelope; errors are deliberately silent drops.
pub fn normalize(agent: &str, input: &[u8]) -> Option<Vec<u8>> {
    if input.is_empty() || input.len() > INPUT_LIMIT || !matches!(agent, "claude_code" | "codex") {
        return None;
    }
    let raw: Value = serde_json::from_slice(input).ok()?;
    let event = raw.get("hook_event_name")?.as_str()?;
    if !matches!(
        event,
        "SessionStart"
            | "UserPromptSubmit"
            | "PreToolUse"
            | "PostToolUse"
            | "Stop"
            | "StopFailure"
            | "SessionEnd"
    ) {
        return None;
    }
    let id = |key: &str| raw.get(key).and_then(Value::as_str).filter(|s| valid_id(s));
    let tool = raw
        .get("tool_name")
        .and_then(Value::as_str)
        .map(|name| match name {
            "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => "edit",
            "Bash" => "shell",
            "exec_command" if agent == "codex" => "shell",
            "Read" | "Grep" | "Glob" | "LS" => "read",
            "WebFetch" | "WebSearch" => "web",
            _ => "other",
        });
    let command_class = (tool == Some("shell")).then(|| {
        classify(
            raw.get("tool_input")
                .and_then(|v| v.get("command"))
                .and_then(Value::as_str)
                .unwrap_or_default(),
        )
    });
    let envelope = Envelope {
        v: 1,
        agent,
        event,
        session_id: id("session_id")?,
        turn_id: id("turn_id"),
        tool_use_id: id("tool_use_id"),
        agent_id: id("agent_id"),
        tool,
        command_class,
    };
    let mut line = serde_json::to_vec(&envelope).ok()?;
    line.push(b'\n');
    (line.len() <= ENVELOPE_LIMIT).then_some(line)
}

/// Ownership at check time only: not an ancestor/ACL/peer-identity/TOCTOU guarantee.
fn owned_socket(socket: &Metadata, parent: &Metadata, effective_uid: u32) -> bool {
    socket.file_type().is_socket()
        && owners_match(socket.uid(), parent.uid(), effective_uid)
        && parent.file_type().is_dir()
        && parent.mode() & 0o7777 == 0o700
}

fn owners_match(socket_uid: u32, parent_uid: u32, effective_uid: u32) -> bool {
    socket_uid == effective_uid && parent_uid == effective_uid
}

fn socket_allowed(path: &Path) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    let (Ok(socket), Ok(parent)) = (fs::symlink_metadata(path), fs::symlink_metadata(parent))
    else {
        return false;
    };
    owned_socket(&socket, &parent, rustix::process::geteuid().as_raw())
}

fn bounded<T: Send + 'static>(
    budget: Duration,
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (tx, rx) = mpsc::sync_channel(1);
    let _worker = thread::Builder::new()
        .spawn(move || {
            let _ = tx.send(work());
        })
        .ok()?;
    rx.recv_timeout(budget).ok()
}

fn forward(agent: &'static str, path: PathBuf, started: Instant) -> Option<()> {
    let input = bounded(
        READ_BUDGET.min(TOTAL_BUDGET.checked_sub(started.elapsed())?),
        || {
            let mut input = Vec::new();
            std::io::stdin()
                .take((INPUT_LIMIT + 1) as u64)
                .read_to_end(&mut input)
                .ok()?;
            Some(input)
        },
    )??;
    let line = normalize(agent, &input)?;
    drop(input);
    if !socket_allowed(&path) {
        return None;
    }
    let mut stream = bounded(
        IO_BUDGET.min(TOTAL_BUDGET.checked_sub(started.elapsed())?),
        move || UnixStream::connect(path).ok(),
    )??;
    stream
        .set_write_timeout(Some(
            IO_BUDGET.min(TOTAL_BUDGET.checked_sub(started.elapsed())?),
        ))
        .ok()?;
    bounded(
        IO_BUDGET.min(TOTAL_BUDGET.checked_sub(started.elapsed())?),
        move || stream.write_all(&line).ok(),
    )??;
    Some(())
}

/// Process entry: all arguments, errors and timeouts fail open without stdout/stderr.
pub fn run(started: Instant) {
    let mut args = std::env::args_os().skip(1);
    let agent = match args.next().as_deref().and_then(|s| s.to_str()) {
        Some("claude-code") => "claude_code",
        Some("codex") => "codex",
        _ => return,
    };
    if args.next().as_deref().and_then(|s| s.to_str()) != Some("--socket") {
        return;
    }
    let Some(path) = args.next().map(PathBuf::from).filter(|p| p.is_absolute()) else {
        return;
    };
    if args.next().is_some() {
        return;
    }
    // The supervisor also caps metadata/JSON work and a blocking Unix connect. Detached
    // workers are terminated by process exit; this library entry is for the hook binary only.
    if let Some(remaining) = TOTAL_BUDGET.checked_sub(started.elapsed()) {
        let _ = bounded(remaining, move || forward(agent, path, started));
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use serde_json::json;

    #[test]
    fn classifier_table_and_precedence() {
        for command in [
            "cargo test",
            "cargo nextest run",
            "npm test",
            "pnpm test",
            "yarn test",
            "bun test",
            "npm run test",
            "pnpm run test",
            "yarn run test",
            "bun run test",
            "npx vitest",
            "npx jest",
            "vitest",
            "jest",
            "pytest -q",
            "go test ./...",
            "swift test",
            "xcodebuild test",
            "make test",
            "cd x && cargo test",
            "FOO=1 npm test",
            "env FOO=1 npm test",
            "cd x; env X=1 pytest",
            "echo x || cargo test",
            "echo x | pytest",
        ] {
            assert_eq!(classify(command), CommandClass::Test, "{command}");
        }
        for command in [
            "git merge branch",
            "git rebase branch",
            "git cherry-pick abc",
            "git pull --rebase",
            "cargo test && git merge branch",
            "git rebase branch; npm test",
        ] {
            assert_eq!(classify(command), CommandClass::GitMerge, "{command}");
        }
        for command in [
            "echo cargo test",
            "echo x & cargo test",
            "echo 'x && cargo test'",
            "echo \"x; pytest\"",
            "cd cargo test",
            "cargo testing",
            "npm run testing",
            "git merger",
            "git pull",
            "git pull --rebase=false",
            "echo git merge",
            "",
            "echo test",
        ] {
            assert_eq!(classify(command), CommandClass::Other, "{command}");
        }
        assert_eq!(
            classify(&format!("{}; cargo test", " ".repeat(4096))),
            CommandClass::Other
        );
        assert_eq!(
            classify(&format!("{}é; cargo test", " ".repeat(4095))),
            CommandClass::Other
        );
    }

    #[test]
    fn exact_input_cap_and_supervised_deadline() {
        let mut input = br#"{"hook_event_name":"Stop","session_id":"s"}"#.to_vec();
        input.resize(INPUT_LIMIT, b' ');
        assert!(normalize("claude_code", &input).is_some());
        input.push(b' ');
        assert!(normalize("claude_code", &input).is_none());
        let started = Instant::now();
        let result = bounded(Duration::from_millis(20), || {
            thread::sleep(Duration::from_millis(100));
            true
        });
        assert!(result.is_none());
        assert!(started.elapsed() < Duration::from_millis(400));
    }

    #[test]
    fn effective_ownership_is_compared_independently() {
        let uid = rustix::process::geteuid().as_raw();
        let other = uid.wrapping_add(1);
        assert!(owners_match(uid, uid, uid));
        assert!(
            !owners_match(other, uid, uid),
            "injected socket owner mismatch"
        );
        assert!(
            !owners_match(uid, other, uid),
            "injected parent owner mismatch"
        );
        assert!(
            !owners_match(uid, uid, other),
            "injected effective UID mismatch"
        );
    }

    #[test]
    fn allowlist_mapping_ids_and_privacy() {
        let mappings = [
            ("Edit", "edit"),
            ("Write", "edit"),
            ("MultiEdit", "edit"),
            ("NotebookEdit", "edit"),
            ("Bash", "shell"),
            ("Read", "read"),
            ("Grep", "read"),
            ("Glob", "read"),
            ("LS", "read"),
            ("WebFetch", "web"),
            ("WebSearch", "web"),
            ("Unrecognized", "other"),
        ];
        for (name, category) in mappings {
            let input = json!({"hook_event_name":"PreToolUse", "session_id":"session:1", "turn_id":"../invalid", "tool_use_id":"tool_1", "agent_id":"agent-1", "tool_name":name, "tool_input":{"command":"cargo test SECRET_COMMAND", "file_path":"SECRET_PATH"}, "prompt":"SECRET_PROMPT", "cwd":"SECRET_CWD", "tool_response":"SECRET_OUTPUT"});
            let line = normalize("claude_code", &serde_json::to_vec(&input).unwrap()).unwrap();
            assert!(line.ends_with(b"\n") && line.len() <= 1024);
            let text = String::from_utf8(line.clone()).unwrap();
            assert!(!text.contains("SECRET"));
            let envelope: Value = serde_json::from_slice(&line).unwrap();
            assert_eq!(envelope["tool"], category);
            assert!(envelope.get("turn_id").is_none());
            assert_eq!(envelope.get("command_class").is_some(), category == "shell");
        }
        for event in [
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "Stop",
            "StopFailure",
            "SessionEnd",
        ] {
            assert!(
                normalize(
                    "codex",
                    &serde_json::to_vec(&json!({"session_id":"s", "hook_event_name":event}))
                        .unwrap()
                )
                .is_some()
            );
        }
        for invalid in [
            json!(null),
            json!(2),
            json!(""),
            json!("bad/id"),
            json!("é"),
            json!("x".repeat(129)),
        ] {
            assert!(
                normalize(
                    "claude_code",
                    &serde_json::to_vec(&json!({"session_id":invalid,"hook_event_name":"Stop"}))
                        .unwrap()
                )
                .is_none()
            );
        }
        assert!(normalize("claude_code", br#"{"hook_event_name":"Stop"}"#).is_none());
        assert!(
            normalize(
                "claude_code",
                br#"{"session_id":"s","hook_event_name":"Notification"}"#
            )
            .is_none()
        );
        let maximal = json!({"session_id":"x".repeat(128),"turn_id":"x".repeat(128),"tool_use_id":"x".repeat(128),"agent_id":"x".repeat(128),"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"git merge x"}});
        assert!(
            normalize("claude_code", &serde_json::to_vec(&maximal).unwrap())
                .unwrap()
                .len()
                <= 1024
        );
        let codex = br#"{"session_id":"s","hook_event_name":"PreToolUse","tool_name":"exec_command","tool_input":{"command":"cargo test"}}"#;
        let codex: Value = serde_json::from_slice(&normalize("codex", codex).unwrap()).unwrap();
        assert_eq!(codex["tool"], "shell");
        assert_eq!(codex["command_class"], "test");
    }
}
