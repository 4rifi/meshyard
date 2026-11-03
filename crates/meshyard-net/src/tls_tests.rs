//! Independent rustls QUIC resumption proof; no sockets or application credentials.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use rustls::{
    ClientConfig, HandshakeKind, RootCertStore, ServerConfig,
    pki_types::{PrivatePkcs8KeyDer, ServerName},
    quic::{ClientConnection, KeyChange, Keys, PacketKey, ServerConnection, Version},
};

const MARKER: &[u8] = b"test-only mutation";
const HEADER: &[u8] = b"test-only authenticated QUIC header";

fn configs() -> (Arc<ClientConfig>, ServerConfig) {
    let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
    let cert = certified.cert.der().clone();
    let key = PrivatePkcs8KeyDer::from(certified.signing_key.serialize_der());
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut roots = RootCertStore::empty();
    roots.add(cert.clone()).unwrap();
    let mut client = ClientConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
    client.enable_early_data = true;
    client.alpn_protocols = vec![b"meshyard/test/1".to_vec()];
    let mut server = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key.into())
        .unwrap();
    server.alpn_protocols = client.alpn_protocols.clone();
    server.max_early_data_size = u32::MAX;
    (Arc::new(client), server)
}

fn client(config: Arc<ClientConfig>) -> ClientConnection {
    ClientConnection::new(
        config,
        Version::V1,
        ServerName::try_from("localhost").unwrap(),
        vec![1, 2, 3],
    )
    .unwrap()
}

// Each write is delivered separately, preserving TLS encryption-level boundaries.
fn finish(client: &mut ClientConnection, server: &mut ServerConnection) -> (Keys, Keys) {
    let (mut client_keys, mut server_keys) = (None, None);
    for _ in 0..32 {
        let mut progress = false;
        let mut bytes = Vec::new();
        if let Some(change) = client.write_hs(&mut bytes) {
            if let KeyChange::OneRtt { keys, .. } = change {
                client_keys = Some(keys);
            }
            progress = true;
        }
        if !bytes.is_empty() {
            server.read_hs(&bytes).unwrap();
            progress = true;
        }
        bytes.clear();
        if let Some(change) = server.write_hs(&mut bytes) {
            if let KeyChange::OneRtt { keys, .. } = change {
                server_keys = Some(keys);
            }
            progress = true;
        }
        if !bytes.is_empty() {
            client.read_hs(&bytes).unwrap();
            progress = true;
        }
        if !progress {
            assert!(!client.is_handshaking());
            assert!(!server.is_handshaking());
            return (client_keys.unwrap(), server_keys.unwrap());
        }
    }
    panic!("in-memory handshake failed to drain within its bounded message budget");
}

fn encrypt(key: &dyn PacketKey) -> Vec<u8> {
    let mut packet = MARKER.to_vec();
    let tag = key.encrypt_in_place(0, HEADER, &mut packet).unwrap();
    packet.extend_from_slice(tag.as_ref());
    packet
}

fn apply(key: &dyn PacketKey, mut packet: Vec<u8>, mutations: &mut usize) {
    assert_eq!(
        key.decrypt_in_place(0, HEADER, &mut packet).unwrap(),
        MARKER
    );
    *mutations += 1;
}

#[test]
fn capable_quic_client_accepts_control_and_rejects_disabled_server_early_data() {
    for accept_early in [true, false] {
        let (config, mut server_config) = configs();
        // Prime a real early-data-capable ticket. Clones preserve the certificate resolver,
        // session storage and ticketer; only the server's early-data limit changes below.
        let mut initial_client = client(config.clone());
        assert!(initial_client.zero_rtt_keys().is_none());
        let mut initial_server =
            ServerConnection::new(Arc::new(server_config.clone()), Version::V1, vec![4, 5, 6])
                .unwrap();
        finish(&mut initial_client, &mut initial_server);
        assert!(initial_client.tls13_tickets_received() > 0);
        assert_eq!(initial_client.handshake_kind(), Some(HandshakeKind::Full));

        server_config.max_early_data_size = if accept_early { u32::MAX } else { 0 };
        let mut resumed_client = client(config);
        // Mandatory capability check in BOTH branches: a ticket was consumed and 0-RTT offered.
        let early_keys = resumed_client
            .zero_rtt_keys()
            .expect("capable resumed client");
        let packet = encrypt(early_keys.packet.as_ref());
        let mut resumed_server =
            ServerConnection::new(Arc::new(server_config), Version::V1, vec![4, 5, 6]).unwrap();
        let mut hello = Vec::new();
        assert!(resumed_client.write_hs(&mut hello).is_none());
        assert!(!hello.is_empty());
        resumed_server.read_hs(&hello).unwrap();
        assert!(resumed_server.is_handshaking());
        let server_early_keys = resumed_server.zero_rtt_keys();
        assert_eq!(server_early_keys.is_some(), accept_early);
        let mut preauth_mutations = 0;
        if let Some(keys) = server_early_keys {
            apply(keys.packet.as_ref(), packet, &mut preauth_mutations);
        }
        assert_eq!(preauth_mutations, usize::from(accept_early));
        let (client_keys, server_keys) = finish(&mut resumed_client, &mut resumed_server);
        assert_eq!(resumed_client.is_early_data_accepted(), accept_early);
        assert_eq!(
            resumed_client.handshake_kind(),
            Some(HandshakeKind::Resumed)
        );
        assert_eq!(
            resumed_server.handshake_kind(),
            Some(HandshakeKind::Resumed)
        );
        if !accept_early {
            assert_eq!(preauth_mutations, 0);
            // Rejected early bytes may legitimately be retransmitted after authenticated 1-RTT.
            let mut authenticated_mutations = 0;
            apply(
                server_keys.remote.packet.as_ref(),
                encrypt(client_keys.local.packet.as_ref()),
                &mut authenticated_mutations,
            );
            assert_eq!(authenticated_mutations, 1);
        }
    }
}
