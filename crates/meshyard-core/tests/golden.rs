#![allow(clippy::unwrap_used, clippy::expect_used)]
use meshyard_core::wire::v1::*;
use serde::{Serialize, de::DeserializeOwned};
fn golden<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(name: &str, value: T) {
    let line = include_str!("golden-v1.txt")
        .lines()
        .find(|line| line.starts_with(&format!("{name} ")))
        .unwrap();
    let hex = line.split_once(' ').unwrap().1;
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    assert_eq!(postcard::to_stdvec(&value).unwrap(), bytes, "{name}");
    let (decoded, rest) = postcard::take_from_bytes::<T>(&bytes).unwrap();
    assert!(rest.is_empty());
    assert_eq!(decoded, value);
}
#[test]
fn every_wire_struct_and_variant_has_fixed_golden_bytes() {
    golden(
        "genesis",
        GenesisV1 {
            v: 1,
            owner: [1; 32],
            name: "G".into(),
            mode: ModeV1::Auto,
            created_ms: 300,
            nonce: [2; 16],
        },
    );
    golden(
        "signed_genesis",
        SignedGenesisV1 {
            genesis_bytes: vec![0xaa, 0xbb],
            sig: [7; 64],
        },
    );
    golden(
        "header",
        EventHeaderV1 {
            v: 1,
            workspace: [3; 16],
            author: [4; 32],
            seq: 300,
            ts_ms: 0,
            membership_epoch: 1,
        },
    );
    golden(
        "signed_event",
        SignedEventV1 {
            signed: vec![0xcc],
            sig: [8; 64],
        },
    );
    golden(
        "member_added",
        EventBodyV1::MemberAdded {
            device: [1; 32],
            display_name: "M".into(),
            role: MemberRoleV1::Member,
        },
    );
    golden(
        "member_removed",
        EventBodyV1::MemberRemoved { device: [1; 32] },
    );
    golden(
        "profile",
        EventBodyV1::ProfileUpdated {
            display_name: "P".into(),
            avatar: 300,
        },
    );
    golden(
        "registered",
        EventBodyV1::AgentRegistered {
            agent_local_id: [0; 16],
            kind: AgentKindV1::ClaudeCode,
            label: "A".into(),
        },
    );
    golden(
        "state",
        EventBodyV1::AgentState {
            agent_local_id: [0; 16],
            state: ActivityStateV1::Testing,
            provenance: ProvenanceV1::Verified {
                source: HookEventV1::PreToolUse,
            },
            tool: Some(ToolCategoryV1::Shell),
            task_label: Some("T".into()),
        },
    );
    golden(
        "state_none",
        EventBodyV1::AgentState {
            agent_local_id: [0; 16],
            state: ActivityStateV1::Idle,
            provenance: ProvenanceV1::Inferred {
                rule: RuleIdV1::Stale,
            },
            tool: None,
            task_label: None,
        },
    );
    golden(
        "gone",
        EventBodyV1::AgentGone {
            agent_local_id: [0; 16],
        },
    );
    golden("role_member", MemberRoleV1::Member);
    for (i, v) in [ModeV1::Auto, ModeV1::Strict].into_iter().enumerate() {
        golden(&format!("mode_{i}"), v);
    }
    for (i, v) in [
        ActivityStateV1::Idle,
        ActivityStateV1::Thinking,
        ActivityStateV1::Building,
        ActivityStateV1::Reviewing,
        ActivityStateV1::Testing,
        ActivityStateV1::Fixing,
        ActivityStateV1::WaitingForInput,
        ActivityStateV1::Merging,
        ActivityStateV1::Finished,
        ActivityStateV1::Error,
    ]
    .into_iter()
    .enumerate()
    {
        golden(&format!("activity_{i}"), v);
    }
    for (i, v) in [
        AgentKindV1::ClaudeCode,
        AgentKindV1::Codex,
        AgentKindV1::Other,
    ]
    .into_iter()
    .enumerate()
    {
        golden(&format!("kind_{i}"), v);
    }
    for (i, v) in [
        ToolCategoryV1::Edit,
        ToolCategoryV1::Shell,
        ToolCategoryV1::Read,
        ToolCategoryV1::Web,
        ToolCategoryV1::Other,
    ]
    .into_iter()
    .enumerate()
    {
        golden(&format!("tool_{i}"), v);
    }
    for (i, v) in [
        HookEventV1::SessionStart,
        HookEventV1::UserPromptSubmit,
        HookEventV1::PreToolUse,
        HookEventV1::PostToolUse,
        HookEventV1::Stop,
        HookEventV1::StopFailure,
        HookEventV1::SessionEnd,
    ]
    .into_iter()
    .enumerate()
    {
        golden(&format!("hook_{i}"), v);
    }
    for (i, v) in [
        RuleIdV1::ToolEdit,
        RuleIdV1::ToolShell,
        RuleIdV1::ToolRead,
        RuleIdV1::ToolWeb,
        RuleIdV1::TurnEnded,
        RuleIdV1::Stale,
    ]
    .into_iter()
    .enumerate()
    {
        golden(&format!("rule_{i}"), v);
    }
    golden(
        "verified",
        ProvenanceV1::Verified {
            source: HookEventV1::Stop,
        },
    );
    golden(
        "inferred",
        ProvenanceV1::Inferred {
            rule: RuleIdV1::TurnEnded,
        },
    );
    let frame = FrameHeaderV1 {
        frame_type: 2,
        len: 256,
    };
    let expected = [0x4d, 0x53, 0x48, 0x59, 1, 2, 0, 0, 1, 0];
    assert_eq!(frame.encode().unwrap(), expected);
    assert_eq!(FrameHeaderV1::decode(expected, &[2]).unwrap(), frame);
}
#[test]
fn frame_rejects_magic_version_type_and_length_before_payload() {
    let good = FrameHeaderV1 {
        frame_type: 2,
        len: 256,
    }
    .encode()
    .unwrap();
    for (i, value) in [(0, 0), (4, 2), (5, 3), (6, 0xff)] {
        let mut bad = good;
        bad[i] = value;
        assert!(FrameHeaderV1::decode(bad, &[2]).is_err());
    }
    assert!(
        FrameHeaderV1 {
            frame_type: 2,
            len: MAX_FRAME as u32 + 1
        }
        .encode()
        .is_err()
    );
}
