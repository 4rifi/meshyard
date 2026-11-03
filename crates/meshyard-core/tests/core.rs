#![allow(clippy::unwrap_used, clippy::expect_used)]
use meshyard_core::{wire::v1::*, *};
use proptest::prelude::*;
fn genesis(owner: DeviceId) -> Genesis {
    Genesis {
        owner,
        name: "Workspace".into(),
        mode: Mode::Strict,
        created_ms: 0,
        nonce: [0; 16],
    }
}
fn event(author: DeviceId, seq: u64, body: EventBody) -> Event {
    Event {
        workspace: WorkspaceId([0; 16]),
        author,
        seq,
        ts_ms: u64::MAX - seq,
        membership_epoch: 0,
        body,
    }
}
fn profile(name: &str) -> EventBody {
    EventBody::ProfileUpdated {
        display_name: name.into(),
        avatar: 7,
    }
}
fn registration(label: &str) -> EventBody {
    EventBody::AgentRegistered {
        agent_local_id: [0; 16],
        kind: AgentKind::Codex,
        label: label.into(),
    }
}
fn activity() -> EventBody {
    EventBody::AgentState {
        agent_local_id: [0; 16],
        state: ActivityState::Testing,
        provenance: Provenance::Verified {
            source: HookEvent::PreToolUse,
        },
        tool: Some(ToolCategory::Shell),
        task_label: Some("raw prompt command /secret/path".into()),
    }
}
fn raw(key: &SecretKey, body: EventBody) -> SignedEventV1 {
    let h = EventHeaderV1 {
        v: 1,
        workspace: [0; 16],
        author: key.device_id().0,
        seq: 1,
        ts_ms: 0,
        membership_epoch: 0,
    };
    let mut signed = postcard::to_stdvec(&h).unwrap();
    signed.extend(postcard::to_stdvec(&EventBodyV1::from(body)).unwrap());
    let sig = key.sign(&domain_message(EVENT_DOMAIN, &signed)).unwrap();
    SignedEventV1 { signed, sig }
}
#[test]
fn signatures_cover_raw_header_body_and_domains() {
    let key = SecretKey::from_bytes(&[42; 32]);
    let signed = sign_event(
        event(key.device_id(), 1, profile("ok")),
        &key,
        &SharingPolicy::default(),
    )
    .unwrap();
    assert!(verify_and_decode(&signed, &Ed25519Verifier).is_ok());
    for index in [1, signed.signed.len() - 1] {
        let mut bad = signed.clone();
        bad.signed[index] ^= 1;
        assert_eq!(
            verify_and_decode(&bad, &Ed25519Verifier),
            Err(CryptoError::Signature)
        );
    }
    let mut bad = signed.clone();
    bad.sig = key
        .sign(&domain_message(GENESIS_DOMAIN, &bad.signed))
        .unwrap();
    assert_eq!(
        verify_and_decode(&bad, &Ed25519Verifier),
        Err(CryptoError::Signature)
    );
    // A body that cannot decode must not be examined before signature verification.
    let mut bad = signed.clone();
    let (_, rest) = postcard::take_from_bytes::<EventHeaderV1>(&bad.signed).unwrap();
    let header_len = bad.signed.len() - rest.len();
    bad.signed.truncate(header_len);
    assert_eq!(
        verify_and_decode(&bad, &Ed25519Verifier),
        Err(CryptoError::Signature)
    );
    for mutation in 0..2 {
        let mut bad = signed.clone();
        if mutation == 0 {
            bad.signed.push(0);
        } else {
            bad.signed.pop();
        }
        bad.sig = key
            .sign(&domain_message(EVENT_DOMAIN, &bad.signed))
            .unwrap();
        assert!(verify_and_decode(&bad, &Ed25519Verifier).is_err());
    }
    let mut outer = postcard::to_stdvec(&signed).unwrap();
    outer.push(0);
    assert!(decode_signed_event(&outer).is_err());
    assert!(decode_signed_event(&outer[..10]).is_err());
}
#[test]
fn genesis_is_raw_owner_anchored_and_domain_separated() {
    let key = SecretKey::from_bytes(&[42; 32]);
    let other = SecretKey::from_bytes(&[43; 32]);
    assert_eq!(
        sign_genesis(genesis(other.device_id()), &key),
        Err(CryptoError::OwnerMismatch)
    );
    let mut g = genesis(key.device_id());
    g.name = "A\n\u{85}😺".repeat(40);
    let signed = sign_genesis(g, &key).unwrap();
    let (id, decoded) = verify_genesis(&signed, &Ed25519Verifier).unwrap();
    assert_eq!(id, workspace_id(&signed.genesis_bytes));
    assert!(validate_received(&decoded.name, 64).is_ok());
    let expected = blake3::hash(&[WORKSPACE_DOMAIN, signed.genesis_bytes.as_slice()].concat());
    assert_eq!(&id.0, &expected.as_bytes()[..16]);
    let mut bad = signed.clone();
    bad.sig = other
        .sign(&domain_message(GENESIS_DOMAIN, &bad.genesis_bytes))
        .unwrap();
    assert_eq!(
        verify_genesis(&bad, &Ed25519Verifier),
        Err(CryptoError::Signature)
    );
    let mut bad = signed.clone();
    bad.sig = key
        .sign(&domain_message(EVENT_DOMAIN, &bad.genesis_bytes))
        .unwrap();
    assert!(verify_genesis(&bad, &Ed25519Verifier).is_err());
    let mut bad = signed.clone();
    bad.genesis_bytes.push(0);
    bad.sig = key
        .sign(&domain_message(GENESIS_DOMAIN, &bad.genesis_bytes))
        .unwrap();
    assert_eq!(
        verify_genesis(&bad, &Ed25519Verifier),
        Err(CryptoError::Encoding)
    );
    let mut raw_genesis = sign_genesis(
        Genesis {
            owner: key.device_id(),
            name: "x".into(),
            mode: Mode::Auto,
            created_ms: 0,
            nonce: [0; 16],
        },
        &key,
    )
    .unwrap();
    let canonical_id = workspace_id(&raw_genesis.genesis_bytes);
    raw_genesis.genesis_bytes.splice(36..37, [0x80, 0]);
    raw_genesis.sig = key
        .sign(&domain_message(GENESIS_DOMAIN, &raw_genesis.genesis_bytes))
        .unwrap();
    let (raw_id, _) = verify_genesis(&raw_genesis, &Ed25519Verifier).unwrap();
    assert_eq!(raw_id, workspace_id(&raw_genesis.genesis_bytes));
    assert_ne!(raw_id, canonical_id);
    let mut other_nonce = decoded;
    other_nonce.nonce[0] = 1;
    assert_ne!(
        workspace_id(&sign_genesis(other_nonce, &key).unwrap().genesis_bytes),
        id
    );
}
#[test]
fn verification_uses_original_bytes_even_when_postcard_would_reencode_differently() {
    let key = SecretKey::from_bytes(&[42; 32]);
    let mut signed = raw(&key, profile("ok"));
    // Postcard accepts a redundant varint for seq=1. It is authenticated and must remain raw.
    signed.signed.splice(49..50, [0x81, 0]);
    signed.sig = key
        .sign(&domain_message(EVENT_DOMAIN, &signed.signed))
        .unwrap();
    assert_eq!(verify_and_decode(&signed, &Ed25519Verifier).unwrap().seq, 1);
    let original = signed.signed.clone();
    assert_eq!(signed.signed, original);
}
#[test]
fn receiver_rejects_fields_without_rewriting_and_sender_sanitizes() {
    let key = SecretKey::from_bytes(&[42; 32]);
    for value in ["bad\n".to_string(), "x".repeat(65), "bad\u{85}".to_string()] {
        let signed = raw(&key, profile(&value));
        let copy = signed.clone();
        let decoded = verify_and_decode(&signed, &Ed25519Verifier).unwrap();
        assert!(decoded.validate_received().is_err());
        assert_eq!(signed, copy);
        let own = sign_event(
            event(key.device_id(), 1, profile(&value)),
            &key,
            &SharingPolicy::default(),
        )
        .unwrap();
        assert!(
            verify_and_decode(&own, &Ed25519Verifier)
                .unwrap()
                .validate_received()
                .is_ok()
        );
    }
    let mut bad = raw(&key, profile("x"));
    let index = bad.signed.len() - 2;
    bad.signed[index] = 0xff;
    bad.sig = key
        .sign(&domain_message(EVENT_DOMAIN, &bad.signed))
        .unwrap();
    assert_eq!(
        verify_and_decode(&bad, &Ed25519Verifier),
        Err(CryptoError::InvalidField)
    );
    assert!(
        sign_event(
            event(key.device_id(), 0, profile("x")),
            &key,
            &SharingPolicy::default()
        )
        .is_err()
    );
}
#[test]
fn policy_drops_free_text_task_but_preserves_user_agent_label_and_structured_fields() {
    let default = SharingPolicy::default();
    let mut expected = activity();
    if let EventBody::AgentState { task_label, .. } = &mut expected {
        *task_label = None;
    }
    assert_eq!(default.filter(activity()), expected);
    assert_eq!(
        SharingPolicy {
            allow_task_label: true
        }
        .filter(activity()),
        activity()
    );
    assert_eq!(
        default.filter(registration("My agent 😺")),
        registration("My agent 😺")
    );
    let wire = postcard::to_stdvec(&EventBodyV1::from(default.filter(activity()))).unwrap();
    for forbidden in ["prompt", "command", "/secret/path"] {
        assert!(
            !wire
                .windows(forbidden.len())
                .any(|w| w == forbidden.as_bytes())
        );
    }
}
#[test]
fn fold_owner_ordinary_events_membership_and_agent_ownership() {
    let owner = DeviceId([1; 32]);
    let member = DeviceId([2; 32]);
    let evil = DeviceId([3; 32]);
    let events = vec![
        event(
            owner,
            1,
            EventBody::MemberAdded {
                device: owner,
                display_name: "ignored".into(),
            },
        ),
        event(
            owner,
            2,
            EventBody::MemberAdded {
                device: member,
                display_name: "M".into(),
            },
        ),
        event(owner, 3, profile("Owner")),
        event(owner, 4, registration("Owner agent")),
        event(owner, 5, activity()),
        event(member, 1, activity()),
        event(
            member,
            2,
            EventBody::AgentGone {
                agent_local_id: [0; 16],
            },
        ),
        event(
            member,
            3,
            EventBody::MemberAdded {
                device: evil,
                display_name: "forged".into(),
            },
        ),
    ];
    let state = fold(&genesis(owner), &events).unwrap();
    assert_eq!(state.epoch, 1);
    assert_eq!(state.members.len(), 2);
    assert_eq!(state.members[&owner].display_name, "Owner");
    assert_eq!(
        state.agents[&(owner, [0; 16])].state,
        ActivityState::Testing
    );
    assert!(!state.agents.contains_key(&(member, [0; 16])));
}
#[test]
fn late_membership_replays_unknown_author_and_pending_is_bounded() {
    let owner = DeviceId([1; 32]);
    let member = DeviceId([2; 32]);
    let g = genesis(owner);
    let mut events = vec![
        event(member, 1, profile("Late")),
        event(member, 2, registration("agent")),
        event(member, 3, activity()),
    ];
    let state = fold(&g, &events).unwrap();
    assert_eq!(state.pending_membership[&member].len(), 3);
    assert_eq!(state.members.len(), 1);
    assert!(state.agents.is_empty());
    events.push(event(
        owner,
        1,
        EventBody::MemberAdded {
            device: member,
            display_name: "M".into(),
        },
    ));
    let state = fold(&g, &events).unwrap();
    assert!(state.pending_membership.is_empty());
    assert_eq!(state.members[&member].display_name, "Late");
    assert_eq!(
        state.agents[&(member, [0; 16])].state,
        ActivityState::Testing
    );
    let events: Vec<_> = (1..=1025)
        .map(|seq| event(member, seq, profile("bounded")))
        .collect();
    let state = fold(&g, &events).unwrap();
    assert!(state.pending_membership.is_empty());
    assert_eq!(state.pending_overflow, vec![member]);
}
#[test]
fn owner_membership_seq_order_and_removed_authors_are_held() {
    let owner = DeviceId([1; 32]);
    let member = DeviceId([2; 32]);
    let events = vec![
        event(owner, 3, EventBody::MemberRemoved { device: member }),
        event(member, 1, profile("never speculative")),
        event(
            owner,
            1,
            EventBody::MemberAdded {
                device: member,
                display_name: "m".into(),
            },
        ),
        event(owner, 2, profile("owner")),
    ];
    let state = fold(&genesis(owner), &events).unwrap();
    assert_eq!(state.epoch, 2);
    assert!(!state.members.contains_key(&member));
    assert_eq!(state.pending_membership[&member].len(), 1);
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn unconfirmed_quota_is_interleaving_invariant(count in 1020u64..1030, split in 0usize..1020) {
        let owner = DeviceId([1;32]);
        let unknown = DeviceId([2;32]);
        let other = DeviceId([3;32]);
        let a: Vec<_> = (1..=count).map(|seq| event(unknown, seq, profile("held"))).collect();
        let b = vec![event(other, 1, profile("other"))];
        let mut baseline = a.clone();
        baseline.extend(b.clone());
        let mut mixed = a[..split].to_vec();
        mixed.extend(b);
        mixed.extend_from_slice(&a[split..]);
        prop_assert_eq!(fold(&genesis(owner), &baseline).unwrap(), fold(&genesis(owner), &mixed).unwrap());
    }
    #[test]
    fn sanitation_always_valid(value in ".*",cap in 0usize..200) { prop_assert!(validate_received(&sanitize_for_signing(&value,cap),cap).is_ok()); }
    #[test]
    fn cross_author_interleavings_preserve_projection(choices in prop::collection::vec(any::<bool>(),0..100), names in prop::collection::vec("[^\\p{Cc}]{0,12}",6)) {
        let owner=DeviceId([1;32]);let member=DeviceId([2;32]);
        let a=vec![event(owner,1,EventBody::MemberAdded { device:member,display_name:names[0].clone() }),event(owner,2,profile(&names[1])),event(owner,3,registration(&names[2])),event(owner,4,activity())];
        let b=vec![event(member,1,profile(&names[3])),event(member,2,registration(&names[4])),event(member,3,activity())];
        let mut baseline=a.clone();baseline.extend(b.clone());let mut ia=0;let mut ib=0;let mut mixed=vec![];
        for choice in choices { if choice && ia<a.len() {mixed.push(a[ia].clone());ia+=1;} else if ib<b.len(){mixed.push(b[ib].clone());ib+=1;} }
        mixed.extend_from_slice(&a[ia..]);mixed.extend_from_slice(&b[ib..]);prop_assert_eq!(fold(&genesis(owner),&baseline).unwrap(),fold(&genesis(owner),&mixed).unwrap());
    }
}

#[test]
fn received_genesis_fields_unknown_variants_and_versions_are_rejected() {
    let key = SecretKey::from_bytes(&[42; 32]);
    for name in ["bad\n".to_string(), "x".repeat(65)] {
        let g = GenesisV1 {
            v: 1,
            owner: key.device_id().0,
            name,
            mode: ModeV1::Auto,
            created_ms: 0,
            nonce: [0; 16],
        };
        let genesis_bytes = postcard::to_stdvec(&g).unwrap();
        let raw = SignedGenesisV1 {
            sig: key
                .sign(&domain_message(GENESIS_DOMAIN, &genesis_bytes))
                .unwrap(),
            genesis_bytes,
        };
        assert_eq!(
            verify_genesis(&raw, &Ed25519Verifier),
            Err(CryptoError::InvalidField)
        );
    }
    let g = GenesisV1 {
        v: 1,
        owner: key.device_id().0,
        name: "x".into(),
        mode: ModeV1::Auto,
        created_ms: 0,
        nonce: [0; 16],
    };
    let mut bytes = postcard::to_stdvec(&g).unwrap();
    bytes[34] = 0xff;
    let raw = SignedGenesisV1 {
        sig: key.sign(&domain_message(GENESIS_DOMAIN, &bytes)).unwrap(),
        genesis_bytes: bytes,
    };
    assert_eq!(
        verify_genesis(&raw, &Ed25519Verifier),
        Err(CryptoError::InvalidField)
    );
    let mut unknown = raw_event_unknown(&key);
    assert_eq!(
        verify_and_decode(&unknown, &Ed25519Verifier),
        Err(CryptoError::Encoding)
    );
    unknown.signed.truncate(20);
    assert_eq!(
        verify_and_decode(&unknown, &Ed25519Verifier),
        Err(CryptoError::Encoding)
    );
}
fn raw_event_unknown(key: &SecretKey) -> SignedEventV1 {
    let header = EventHeaderV1 {
        v: 1,
        workspace: [0; 16],
        author: key.device_id().0,
        seq: 1,
        ts_ms: 0,
        membership_epoch: 0,
    };
    let mut signed = postcard::to_stdvec(&header).unwrap();
    signed.push(99);
    SignedEventV1 {
        sig: key.sign(&domain_message(EVENT_DOMAIN, &signed)).unwrap(),
        signed,
    }
}
