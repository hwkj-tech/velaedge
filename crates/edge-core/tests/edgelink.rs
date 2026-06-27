use edge_core::{
    decode_edgelink_frame, encode_edgelink_frame, EdgeLinkMessage, EdgeLinkMessageKind,
    EdgeLinkPayload,
};

#[test]
fn edgelink_frame_round_trips_hello_message() {
    let message = EdgeLinkMessage::hello(
        "edge-dev",
        "runtime-dev",
        "0.1.0",
        Some("2026.06.26-001".to_string()),
        vec!["protocol:modbus-tcp".to_string(), "local-store:rocksdb".to_string()],
    );

    let encoded = encode_edgelink_frame(&message).expect("frame should encode");
    let decoded = decode_edgelink_frame(&encoded).expect("frame should decode");

    assert_eq!(decoded.message_id, message.message_id);
    assert_eq!(decoded.sequence, message.sequence);
    assert_eq!(decoded.kind, EdgeLinkMessageKind::Hello);
    assert_eq!(decoded.edge_id, "edge-dev");
    assert_eq!(decoded.runtime_id.as_deref(), Some("runtime-dev"));

    let EdgeLinkPayload::Hello(hello) = decoded.payload else {
        panic!("expected hello payload");
    };
    assert_eq!(hello.runtime_version, "0.1.0");
    assert_eq!(hello.applied_config_version.as_deref(), Some("2026.06.26-001"));
    assert_eq!(hello.capabilities.len(), 2);
}

#[test]
fn edgelink_ack_references_received_message() {
    let hello = EdgeLinkMessage::hello("edge-dev", "runtime-dev", "0.1.0", None, Vec::new());
    let ack = EdgeLinkMessage::ack("edge-dev", "runtime-dev", hello.message_id, hello.sequence);

    assert_eq!(ack.kind, EdgeLinkMessageKind::Ack);
    assert_eq!(ack.edge_id, "edge-dev");
    assert_eq!(ack.runtime_id.as_deref(), Some("runtime-dev"));

    let EdgeLinkPayload::Ack(payload) = ack.payload else {
        panic!("expected ack payload");
    };
    assert_eq!(payload.ack_message_id, hello.message_id);
    assert_eq!(payload.ack_sequence, hello.sequence);
    assert!(payload.accepted);
}

#[test]
fn edgelink_decode_rejects_incomplete_frames() {
    let message = EdgeLinkMessage::hello("edge-dev", "runtime-dev", "0.1.0", None, Vec::new());
    let mut encoded = encode_edgelink_frame(&message).expect("frame should encode");
    encoded.truncate(encoded.len() - 1);

    let error = decode_edgelink_frame(&encoded).expect_err("incomplete frame should fail");
    assert!(error.to_string().contains("incomplete EdgeLink frame"));
}

#[test]
fn edgelink_decode_rejects_invalid_json() {
    let mut frame = Vec::new();
    frame.extend_from_slice(&(4_u32.to_be_bytes()));
    frame.extend_from_slice(b"nope");

    let error = decode_edgelink_frame(&frame).expect_err("invalid json should fail");
    assert!(error.to_string().contains("invalid EdgeLink frame JSON"));
}
