#![allow(clippy::unwrap_used, clippy::expect_used)]
use meshyard_agents::ingest::{Envelope, parse};
use meshyard_agents::state::{StateMachine, is_stale};
use meshyard_core::{
    ActivityState as S, AgentKind, HookEvent as H, Provenance as P, RuleId as R, ToolCategory as T,
};
use serde_json::{Value, json};
use std::time::Duration;

fn envelope(event: &str, session: &str, extra: Value) -> Envelope {
    let mut raw = json!({"v":1,"agent":"claude_code","event":event,"session_id":session});
    for (key, value) in extra.as_object().unwrap() {
        raw[key] = value.clone();
    }
    let mut line = serde_json::to_vec(&raw).unwrap();
    line.push(b'\n');
    parse(&line).unwrap()
}
fn at(n: u64) -> Duration {
    Duration::from_secs(n)
}
fn machine() -> StateMachine {
    StateMachine::new(AgentKind::ClaudeCode)
}
fn prompt(m: &mut StateMachine, session: &str, turn: &str, now: u64) {
    assert_eq!(
        m.apply(
            &envelope("UserPromptSubmit", session, json!({"turn_id":turn})),
            at(now)
        ),
        Some((
            S::Thinking,
            P::Verified {
                source: H::UserPromptSubmit
            },
            None
        ))
    );
}

#[test]
fn official_doc_derived_fixtures_have_exact_golden_states() {
    const BASE: &str = "fixtures/claude-code-doc-derived-2026-11-03";
    let cases = [
        (
            "SessionStart",
            S::Idle,
            P::Verified {
                source: H::SessionStart,
            },
            None,
        ),
        (
            "UserPromptSubmit",
            S::Thinking,
            P::Verified {
                source: H::UserPromptSubmit,
            },
            None,
        ),
        (
            "PreToolUse",
            S::Building,
            P::Inferred { rule: R::ToolEdit },
            Some(T::Edit),
        ),
        ("Stop", S::Finished, P::Verified { source: H::Stop }, None),
        (
            "StopFailure",
            S::Error,
            P::Verified {
                source: H::StopFailure,
            },
            None,
        ),
        (
            "SessionEnd",
            S::Idle,
            P::Verified {
                source: H::SessionEnd,
            },
            None,
        ),
    ];
    for (name, state, provenance, tool) in cases {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join(BASE)
            .join(format!("{name}.json"));
        let raw = std::fs::read(path).unwrap();
        let line = meshyard_hook::normalize("claude_code", &raw).unwrap();
        let input = parse(&line).unwrap();
        assert_eq!(
            input.event(),
            serde_json::from_value::<H>(json!(name)).unwrap()
        );
        assert_eq!(input.agent(), AgentKind::ClaudeCode);
        let mut m = machine();
        if !matches!(name, "SessionStart" | "UserPromptSubmit" | "SessionEnd") {
            m.apply(&envelope("UserPromptSubmit", "abc123", json!({})), at(0));
        }
        assert_eq!(
            m.apply(&input, at(1)),
            Some((state, provenance, tool)),
            "{name}"
        );
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(BASE)
        .join("PostToolUse.json");
    let input =
        parse(&meshyard_hook::normalize("claude_code", &std::fs::read(path).unwrap()).unwrap())
            .unwrap();
    let mut m = machine();
    prompt(&mut m, "abc123", "t", 0);
    assert_eq!(m.apply(&input, at(1)), None);
    assert_eq!(m.current().unwrap().0, S::Thinking);
}

#[test]
fn every_tool_rule_and_no_change_rule_uses_frozen_core_enums() {
    for (tool, class, state, rule) in [
        ("edit", "other", S::Building, R::ToolEdit),
        ("shell", "test", S::Testing, R::ToolShell),
        ("shell", "git_merge", S::Merging, R::ToolShell),
        ("shell", "other", S::Building, R::ToolShell),
        ("read", "other", S::Thinking, R::ToolRead),
        ("web", "other", S::Thinking, R::ToolWeb),
    ] {
        let mut m = machine();
        prompt(&mut m, "s", "t", 0);
        let result = m
            .apply(
                &envelope(
                    "PreToolUse",
                    "s",
                    json!({"tool":tool,"command_class":class,"tool_use_id":"u"}),
                ),
                at(1),
            )
            .unwrap();
        assert_eq!(result.0, state);
        assert_eq!(result.1, P::Inferred { rule });
        assert!(result.2.is_some());
    }
    let mut m = machine();
    prompt(&mut m, "s", "t", 0);
    for event in ["PreToolUse", "PostToolUse"] {
        assert_eq!(
            m.apply(&envelope(event, "s", json!({"tool":"other"})), at(1)),
            None
        );
        assert_eq!(m.current().unwrap().0, S::Thinking);
    }
}

#[test]
fn terminal_ordering_ended_turns_and_turn_start_gate() {
    let mut m = machine();
    assert_eq!(
        m.apply(&envelope("PreToolUse", "s", json!({"tool":"edit"})), at(0)),
        None
    );
    m.apply(&envelope("SessionStart", "s", json!({})), at(0));
    for event in ["PreToolUse", "PostToolUse", "Stop", "StopFailure"] {
        assert_eq!(
            m.apply(&envelope(event, "s", json!({"tool":"edit"})), at(1)),
            None
        );
    }
    prompt(&mut m, "s", "old", 2);
    assert_eq!(
        m.apply(&envelope("Stop", "s", json!({"turn_id":"old"})), at(3))
            .unwrap()
            .0,
        S::Finished
    );
    assert_eq!(
        m.apply(&envelope("PreToolUse", "s", json!({"tool":"edit"})), at(4)),
        None
    );
    prompt(&mut m, "s", "new", 5);
    for event in ["PreToolUse", "Stop", "UserPromptSubmit"] {
        assert_eq!(
            m.apply(
                &envelope(event, "s", json!({"turn_id":"old","tool":"edit"})),
                at(6)
            ),
            None
        );
    }
    assert_eq!(m.current().unwrap().0, S::Thinking);
    assert_eq!(
        m.apply(
            &envelope("StopFailure", "s", json!({"turn_id":"new"})),
            at(7)
        )
        .unwrap()
        .0,
        S::Error
    );
    assert_eq!(m.apply(&envelope("Stop", "s", json!({})), at(8)), None);
    assert_eq!(
        m.apply(&envelope("SessionEnd", "s", json!({})), at(8)),
        Some((
            S::Idle,
            P::Verified {
                source: H::SessionEnd
            },
            None
        ))
    );
}

#[test]
fn duplicate_context_includes_subagent_and_survives_a_new_turn() {
    let mut m = machine();
    prompt(&mut m, "s", "t1", 0);
    let a = envelope(
        "PreToolUse",
        "s",
        json!({"tool":"shell","command_class":"test","tool_use_id":"u","agent_id":"a"}),
    );
    assert_eq!(m.apply(&a, at(1)).unwrap().0, S::Testing);
    assert_eq!(m.apply(&a, at(2)), None);
    let b = envelope(
        "PreToolUse",
        "s",
        json!({"tool":"edit","tool_use_id":"u","agent_id":"b"}),
    );
    assert_eq!(m.apply(&b, at(3)).unwrap().0, S::Building);
    assert_eq!(m.session_count(), 1);
    assert_eq!(
        m.apply(
            &envelope(
                "PostToolUse",
                "s",
                json!({"tool_use_id":"u","agent_id":"b"})
            ),
            at(4)
        ),
        None
    );
    m.apply(&envelope("Stop", "s", json!({})), at(5));
    prompt(&mut m, "s", "t2", 6);
    assert!(m.apply(&a, at(7)).is_none());
    assert!(
        m.apply(
            &envelope(
                "PreToolUse",
                "s",
                json!({"tool":"edit","tool_use_id":"new","agent_id":"a"})
            ),
            at(8)
        )
        .is_some()
    );
    // Real documented envelopes need not have turn IDs: subsequent turns still work.
    let mut m = machine();
    for now in [0, 2, 4] {
        assert!(
            m.apply(&envelope("UserPromptSubmit", "s", json!({})), at(now))
                .is_some()
        );
        assert!(
            m.apply(&envelope("Stop", "s", json!({})), at(now + 1))
                .is_some()
        );
    }
}

#[test]
fn bounded_duplicate_fifo_retains_exactly_sixty_four_keys() {
    let mut m = machine();
    prompt(&mut m, "s", "t", 0);
    let tool = |n| {
        envelope(
            "PreToolUse",
            "s",
            json!({"tool":"edit","tool_use_id":format!("u{n}")}),
        )
    };
    for n in 0..64 {
        assert!(m.apply(&tool(n), at(n + 1)).is_some());
    }
    assert!(m.apply(&tool(0), at(65)).is_none());
    assert!(m.apply(&tool(64), at(66)).is_some());
    assert!(m.apply(&tool(0), at(67)).is_some());
}

#[test]
fn bounded_ended_turn_fifo_retains_exactly_sixteen_ids() {
    let mut m = machine();
    for n in 0..17 {
        prompt(&mut m, "s", &format!("t{n}"), n * 2);
        assert!(
            m.apply(
                &envelope("Stop", "s", json!({"turn_id":format!("t{n}")})),
                at(n * 2 + 1)
            )
            .is_some()
        );
    }
    assert!(
        m.apply(
            &envelope("UserPromptSubmit", "s", json!({"turn_id":"t1"})),
            at(40)
        )
        .is_none()
    );
    prompt(&mut m, "s", "t0", 41);
}

#[test]
fn sessions_are_lru_bounded_and_latest_session_wins() {
    let mut m = machine();
    for n in 0..32 {
        prompt(&mut m, &format!("s{n}"), "t", n);
    }
    assert_eq!(m.session_count(), 32);
    let edit = envelope("PreToolUse", "s0", json!({"tool":"edit","tool_use_id":"u"}));
    assert_eq!(m.apply(&edit, at(33)).unwrap().0, S::Building);
    assert_eq!(m.current().unwrap().0, S::Building);
    prompt(&mut m, "s32", "t", 34);
    assert_eq!(m.session_count(), 32);
    assert_eq!(m.session_state("s1"), None);
    assert_eq!(m.session_state("s0"), Some(S::Building));
    assert_eq!(m.current().unwrap().0, S::Thinking);
}

#[test]
fn finished_timeout_uses_fake_clock_without_stale_wire_events() {
    let mut m = machine();
    prompt(&mut m, "s", "t", 0);
    m.apply(&envelope("Stop", "s", json!({})), at(10));
    assert_eq!(m.tick(at(69)), None);
    assert_eq!(
        m.tick(at(70)),
        Some((S::Idle, P::Inferred { rule: R::TurnEnded }, None))
    );
    assert_eq!(m.tick(at(71)), None);
    prompt(&mut m, "s", "t2", 72);
    assert_eq!(m.tick(at(1000)), None);
    assert_eq!(m.current().unwrap().0, S::Thinking);
    assert!(!is_stale(S::Thinking, at(0), at(600)));
    assert!(is_stale(S::Thinking, at(0), at(601)));
    assert!(!is_stale(S::Idle, at(0), at(9999)));
    assert!(!is_stale(S::Finished, at(0), at(9999)));
    assert!(!is_stale(S::Building, at(10), at(9)));
    assert!(is_stale(S::Error, at(0), at(601)));
}

#[test]
fn old_finished_session_cannot_override_newer_session_on_tick() {
    let mut m = machine();
    prompt(&mut m, "old", "t", 0);
    m.apply(&envelope("Stop", "old", json!({})), at(1));
    prompt(&mut m, "new", "t", 2);
    assert_eq!(m.tick(at(61)), None);
    assert_eq!(m.session_state("old"), Some(S::Idle));
    assert_eq!(m.current().unwrap().0, S::Thinking);
}

#[test]
fn parser_drops_unknown_events_and_ignores_unknown_private_fields() {
    let base = json!({"v":1,"agent":"claude_code","event":"UserPromptSubmit","session_id":"s"});
    let parse_value = |value: &Value| {
        let mut bytes = serde_json::to_vec(value).unwrap();
        bytes.push(b'\n');
        parse(&bytes)
    };
    let valid = parse_value(&base).unwrap();
    let mut raw = base.clone();
    raw["prompt"] = json!("PRIVATE_PROMPT");
    raw["cwd"] = json!("PRIVATE_PATH");
    raw["request_status"] = json!("Ok");
    raw["turn_id"] = json!("bad/id");
    assert_eq!(parse_value(&raw).unwrap(), valid);
    assert!(!format!("{:?}", parse_value(&raw).unwrap()).contains("PRIVATE"));
    for name in [
        "Notification",
        "PermissionRequest",
        "PostToolUseFailure",
        "PostModelRequest",
        "unknown",
    ] {
        raw["event"] = json!(name);
        assert!(parse_value(&raw).is_err());
    }
    for id in [
        json!(""),
        json!("x".repeat(129)),
        json!("é"),
        json!("bad/path"),
        json!(null),
    ] {
        raw = base.clone();
        raw["session_id"] = id;
        assert!(parse_value(&raw).is_err());
    }
    raw = base.clone();
    raw["session_id"] = json!("A-z_0.:-".repeat(16));
    assert!(parse_value(&raw).is_ok());
    assert!(parse(&vec![b'x'; 1025]).is_err());
    assert!(parse(b"{}\n{}\n").is_err());
    assert!(parse(b"{}").is_err());
    let mut m = machine();
    let codex = envelope("UserPromptSubmit", "s", json!({"agent":"codex"}));
    assert_eq!(m.apply(&codex, at(0)), None);
    assert_eq!(m.session_count(), 0);
}
