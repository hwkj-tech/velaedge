use std::net::Ipv4Addr;
use std::time::Duration;

use bacnet_encoding::apdu::{
    self, encode_apdu, Apdu, ComplexAck, RejectPdu, SimpleAck, UnconfirmedRequest,
};
use bacnet_network::layer::NetworkLayer;
use bacnet_services::common::BACnetPropertyValue;
use bacnet_services::cov::{COVNotificationRequest, SubscribeCOVRequest};
use bacnet_services::rpm::{
    ReadAccessResult, ReadPropertyMultipleACK, ReadPropertyMultipleRequest, ReadResultElement,
};
use bacnet_services::write_property::WritePropertyRequest;
use bacnet_transport::bip::BipTransport;
use bacnet_types::enums::{
    ConfirmedServiceChoice, NetworkPriority, ObjectType, PropertyIdentifier, RejectReason,
    UnconfirmedServiceChoice,
};
use bacnet_types::primitives::ObjectIdentifier;
use bytes::{Bytes, BytesMut};
use edge_core::{
    BacnetCovSettings, BacnetForeignDeviceSettings, BacnetIpConnectionSettings, BacnetPointOptions,
    DataQualityCode, PointAccess, PointAddress, ProtocolConnection, TelemetryPointMapping,
    TelemetryType, TelemetryValue,
};
use edge_runtime::{BacnetIpAdapter, ProtocolAdapter, ProtocolCommandAdapter};
use tokio::time::{sleep, timeout};

fn point(point_id: &str) -> TelemetryPointMapping {
    TelemetryPointMapping::new(
        point_id,
        "ahu-1",
        "supply_air_temperature",
        "bacnet-main",
        PointAddress::bacnet(42, "analog_input", 1, "present_value"),
        TelemetryType::Float,
    )
}

fn real_value(value: f32) -> Vec<u8> {
    let mut encoded = vec![0x44];
    encoded.extend_from_slice(&value.to_be_bytes());
    encoded
}

#[tokio::test]
async fn bacnet_ip_writes_commandable_point_with_configured_priority() {
    let transport = BipTransport::new(Ipv4Addr::LOCALHOST, 0, Ipv4Addr::BROADCAST);
    let mut network = NetworkLayer::new(transport);
    let mut receiver = network.start().await.expect("start BACnet/IP device");
    let device_mac = network.local_mac().to_vec();
    let device_port = u16::from_be_bytes([device_mac[4], device_mac[5]]);

    let server = tokio::spawn(async move {
        let received = timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("BACnet/IP WriteProperty timed out")
            .expect("BACnet/IP receive channel closed");
        let Apdu::ConfirmedRequest(request) =
            apdu::decode_apdu(received.apdu.clone()).expect("decode WriteProperty APDU")
        else {
            panic!("expected confirmed BACnet WriteProperty request");
        };
        assert_eq!(
            request.service_choice,
            ConfirmedServiceChoice::WRITE_PROPERTY
        );

        let write = WritePropertyRequest::decode(&request.service_request)
            .expect("decode WriteProperty request");
        assert_eq!(
            write.object_identifier,
            ObjectIdentifier::new_addressable(ObjectType::ANALOG_VALUE, 7)
                .expect("analog value object")
        );
        assert_eq!(write.property_identifier, PropertyIdentifier::PRESENT_VALUE);
        assert_eq!(write.property_array_index, None);
        assert_eq!(write.property_value, real_value(42.5));
        assert_eq!(write.priority, Some(8));

        let mut response = BytesMut::new();
        encode_apdu(
            &mut response,
            &Apdu::SimpleAck(SimpleAck {
                invoke_id: request.invoke_id,
                service_choice: ConfirmedServiceChoice::WRITE_PROPERTY,
            }),
        )
        .expect("encode WriteProperty SimpleAck");
        network
            .send_apdu(
                &response,
                &received.source_mac,
                false,
                NetworkPriority::NORMAL,
            )
            .await
            .expect("send WriteProperty SimpleAck");
        network.stop().await.expect("stop BACnet/IP device");
    });

    let settings = BacnetIpConnectionSettings {
        bind_address: Ipv4Addr::LOCALHOST.to_string(),
        apdu_timeout_ms: 1_000,
        apdu_retries: 0,
        ..BacnetIpConnectionSettings::default()
    };
    let connection = ProtocolConnection::bacnet_ip(
        "bacnet-main",
        Some(format!("127.0.0.1:{device_port}")),
        settings,
    );
    let mapping = TelemetryPointMapping::new(
        "supply_temperature_setpoint",
        "ahu-1",
        "supply_air_temperature_setpoint",
        "bacnet-main",
        PointAddress::bacnet(42, "analog_value", 7, "present_value"),
        TelemetryType::Float,
    )
    .with_access(PointAccess::ReadWrite)
    .with_bacnet_options(BacnetPointOptions { write_priority: 8 });
    let mut adapter =
        BacnetIpAdapter::new(connection, vec![mapping.clone()]).expect("create BACnet adapter");

    let result = adapter
        .write_point(&mapping, TelemetryValue::Float(42.5))
        .await
        .expect("write commandable BACnet point");
    assert_eq!(result.point_id, "supply_temperature_setpoint");
    assert_eq!(result.value, TelemetryValue::Float(42.5));
    assert!(!result.verified);
    assert_eq!(adapter.connection_generation(), 1);

    server.await.expect("BACnet/IP device task");
}

#[tokio::test]
async fn bacnet_ip_batches_duplicate_properties_and_reuses_udp_session() {
    let transport = BipTransport::new(Ipv4Addr::LOCALHOST, 0, Ipv4Addr::BROADCAST);
    let mut network = NetworkLayer::new(transport);
    let mut receiver = network.start().await.expect("start BACnet/IP device");
    let device_mac = network.local_mac().to_vec();
    let device_port = u16::from_be_bytes([device_mac[4], device_mac[5]]);

    let server = tokio::spawn(async move {
        for expected_value in [72.5_f32, 73.25_f32] {
            let received = timeout(Duration::from_secs(2), receiver.recv())
                .await
                .expect("BACnet/IP device timed out")
                .expect("BACnet/IP receive channel closed");
            let decoded = apdu::decode_apdu(received.apdu.clone()).expect("decode request APDU");
            let Apdu::ConfirmedRequest(request) = decoded else {
                panic!("expected confirmed BACnet request");
            };
            assert_eq!(
                request.service_choice,
                ConfirmedServiceChoice::READ_PROPERTY_MULTIPLE
            );

            let rpm = ReadPropertyMultipleRequest::decode(&request.service_request)
                .expect("decode ReadPropertyMultiple request");
            assert_eq!(rpm.list_of_read_access_specs.len(), 1);
            assert_eq!(
                rpm.list_of_read_access_specs[0]
                    .list_of_property_references
                    .len(),
                1,
                "duplicate logical mappings must share one BACnet property read"
            );

            let ack = ReadPropertyMultipleACK {
                list_of_read_access_results: rpm
                    .list_of_read_access_specs
                    .into_iter()
                    .map(|spec| ReadAccessResult {
                        object_identifier: spec.object_identifier,
                        list_of_results: spec
                            .list_of_property_references
                            .into_iter()
                            .map(|property| ReadResultElement {
                                property_identifier: property.property_identifier,
                                property_array_index: property.property_array_index,
                                property_value: Some(real_value(expected_value)),
                                error: None,
                            })
                            .collect(),
                    })
                    .collect(),
            };
            let mut service = BytesMut::new();
            ack.encode(&mut service);
            let response = Apdu::ComplexAck(ComplexAck {
                segmented: false,
                more_follows: false,
                invoke_id: request.invoke_id,
                sequence_number: None,
                proposed_window_size: None,
                service_choice: ConfirmedServiceChoice::READ_PROPERTY_MULTIPLE,
                service_ack: Bytes::from(service.to_vec()),
            });
            let mut encoded = BytesMut::new();
            encode_apdu(&mut encoded, &response).expect("encode response APDU");
            network
                .send_apdu(
                    &encoded,
                    &received.source_mac,
                    false,
                    NetworkPriority::NORMAL,
                )
                .await
                .expect("send BACnet/IP response");
        }
        network.stop().await.expect("stop BACnet/IP device");
    });

    let settings = BacnetIpConnectionSettings {
        bind_address: Ipv4Addr::LOCALHOST.to_string(),
        apdu_timeout_ms: 1_000,
        apdu_retries: 0,
        ..BacnetIpConnectionSettings::default()
    };
    let connection = ProtocolConnection::bacnet_ip(
        "bacnet-main",
        Some(format!("127.0.0.1:{device_port}")),
        settings,
    );
    let mut adapter = BacnetIpAdapter::new(
        connection,
        vec![
            point("supply_temperature"),
            point("supply_temperature_shadow"),
        ],
    )
    .expect("create BACnet/IP adapter");

    let first = adapter.read_telemetry().await.expect("first BACnet read");
    assert_eq!(first.len(), 2);
    assert!(first.iter().all(|sample| {
        sample.value == TelemetryValue::Float(72.5)
            && sample.quality_code == Some(DataQualityCode::Good)
    }));
    assert_eq!(adapter.connection_generation(), 1);

    let second = adapter.read_telemetry().await.expect("second BACnet read");
    assert!(second
        .iter()
        .all(|sample| sample.value == TelemetryValue::Float(73.25)));
    assert_eq!(
        adapter.connection_generation(),
        1,
        "normal collection cycles must reuse the BACnet/IP client"
    );

    server.await.expect("BACnet/IP device task");
}

#[tokio::test]
async fn bacnet_ip_keeps_identical_object_addresses_isolated_per_device() {
    let transport = BipTransport::new(Ipv4Addr::LOCALHOST, 0, Ipv4Addr::BROADCAST);
    let mut network = NetworkLayer::new(transport);
    let mut receiver = network.start().await.expect("start BACnet/IP device");
    let device_mac = network.local_mac().to_vec();
    let device_port = u16::from_be_bytes([device_mac[4], device_mac[5]]);

    let server = tokio::spawn(async move {
        for expected_value in [21.0_f32, 32.0_f32] {
            let received = timeout(Duration::from_secs(2), receiver.recv())
                .await
                .expect("BACnet/IP device timed out")
                .expect("BACnet/IP receive channel closed");
            let Apdu::ConfirmedRequest(request) =
                apdu::decode_apdu(received.apdu.clone()).expect("decode request APDU")
            else {
                panic!("expected confirmed BACnet request");
            };
            let rpm = ReadPropertyMultipleRequest::decode(&request.service_request)
                .expect("decode ReadPropertyMultiple request");
            let ack = ReadPropertyMultipleACK {
                list_of_read_access_results: rpm
                    .list_of_read_access_specs
                    .into_iter()
                    .map(|spec| ReadAccessResult {
                        object_identifier: spec.object_identifier,
                        list_of_results: spec
                            .list_of_property_references
                            .into_iter()
                            .map(|property| ReadResultElement {
                                property_identifier: property.property_identifier,
                                property_array_index: property.property_array_index,
                                property_value: Some(real_value(expected_value)),
                                error: None,
                            })
                            .collect(),
                    })
                    .collect(),
            };
            let mut service = BytesMut::new();
            ack.encode(&mut service);
            let mut encoded = BytesMut::new();
            encode_apdu(
                &mut encoded,
                &Apdu::ComplexAck(ComplexAck {
                    segmented: false,
                    more_follows: false,
                    invoke_id: request.invoke_id,
                    sequence_number: None,
                    proposed_window_size: None,
                    service_choice: ConfirmedServiceChoice::READ_PROPERTY_MULTIPLE,
                    service_ack: service.freeze(),
                }),
            )
            .expect("encode response APDU");
            network
                .send_apdu(
                    &encoded,
                    &received.source_mac,
                    false,
                    NetworkPriority::NORMAL,
                )
                .await
                .expect("send BACnet/IP response");
        }
        network.stop().await.expect("stop BACnet/IP device");
    });

    let settings = BacnetIpConnectionSettings {
        bind_address: Ipv4Addr::LOCALHOST.to_string(),
        apdu_timeout_ms: 1_000,
        apdu_retries: 0,
        ..BacnetIpConnectionSettings::default()
    };
    let connection = ProtocolConnection::bacnet_ip(
        "bacnet-main",
        Some(format!("127.0.0.1:{device_port}")),
        settings,
    );
    let mut device_42 = point("device-42-temperature");
    device_42.device_id = "ahu-42".to_string();
    let mut device_43 = point("device-43-temperature");
    device_43.device_id = "ahu-43".to_string();
    device_43.address = PointAddress::bacnet(43, "analog_input", 1, "present_value");
    let mut adapter = BacnetIpAdapter::new(connection, vec![device_42, device_43])
        .expect("create BACnet/IP adapter");

    let samples = adapter
        .read_telemetry()
        .await
        .expect("read two BACnet devices");
    assert_eq!(samples.len(), 2);
    assert_eq!(
        samples
            .iter()
            .find(|sample| sample.device_id == "ahu-42")
            .expect("device 42 sample")
            .value,
        TelemetryValue::Float(21.0)
    );
    assert_eq!(
        samples
            .iter()
            .find(|sample| sample.device_id == "ahu-43")
            .expect("device 43 sample")
            .value,
        TelemetryValue::Float(32.0)
    );

    server.await.expect("BACnet/IP device task");
}

#[tokio::test]
async fn bacnet_ip_registers_as_foreign_device_with_real_bbmd() {
    let mut transport = BipTransport::new(Ipv4Addr::LOCALHOST, 0, Ipv4Addr::BROADCAST);
    transport.enable_bbmd(Vec::new());
    let mut network = NetworkLayer::new(transport);
    let mut receiver = network.start().await.expect("start BACnet/IP BBMD");
    let bbmd_mac = network.local_mac().to_vec();
    let bbmd_port = u16::from_be_bytes([bbmd_mac[4], bbmd_mac[5]]);
    let bbmd_state = network
        .transport()
        .bbmd_state()
        .expect("BBMD state is enabled")
        .clone();

    let server = tokio::spawn(async move {
        let received = timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("BACnet/IP BBMD device timed out")
            .expect("BACnet/IP receive channel closed");
        let decoded = apdu::decode_apdu(received.apdu.clone()).expect("decode request APDU");
        let Apdu::ConfirmedRequest(request) = decoded else {
            panic!("expected confirmed BACnet request");
        };
        let rpm = ReadPropertyMultipleRequest::decode(&request.service_request)
            .expect("decode ReadPropertyMultiple request");
        let ack = ReadPropertyMultipleACK {
            list_of_read_access_results: rpm
                .list_of_read_access_specs
                .into_iter()
                .map(|spec| ReadAccessResult {
                    object_identifier: spec.object_identifier,
                    list_of_results: spec
                        .list_of_property_references
                        .into_iter()
                        .map(|property| ReadResultElement {
                            property_identifier: property.property_identifier,
                            property_array_index: property.property_array_index,
                            property_value: Some(real_value(21.5)),
                            error: None,
                        })
                        .collect(),
                })
                .collect(),
        };
        let mut service = BytesMut::new();
        ack.encode(&mut service);
        let response = Apdu::ComplexAck(ComplexAck {
            segmented: false,
            more_follows: false,
            invoke_id: request.invoke_id,
            sequence_number: None,
            proposed_window_size: None,
            service_choice: ConfirmedServiceChoice::READ_PROPERTY_MULTIPLE,
            service_ack: Bytes::from(service.to_vec()),
        });
        let mut encoded = BytesMut::new();
        encode_apdu(&mut encoded, &response).expect("encode response APDU");
        network
            .send_apdu(
                &encoded,
                &received.source_mac,
                false,
                NetworkPriority::NORMAL,
            )
            .await
            .expect("send BACnet/IP response");
        network.stop().await.expect("stop BACnet/IP BBMD");
    });

    let settings = BacnetIpConnectionSettings {
        bind_address: Ipv4Addr::LOCALHOST.to_string(),
        apdu_timeout_ms: 1_000,
        apdu_retries: 0,
        foreign_device: Some(BacnetForeignDeviceSettings {
            bbmd_address: format!("127.0.0.1:{bbmd_port}"),
            ttl_seconds: 120,
        }),
        ..BacnetIpConnectionSettings::default()
    };
    let connection = ProtocolConnection::bacnet_ip(
        "bacnet-bbmd",
        Some(format!("127.0.0.1:{bbmd_port}")),
        settings,
    );
    let mut mapping = point("bbmd_temperature");
    mapping.protocol_connection_id = "bacnet-bbmd".to_string();
    let mut adapter =
        BacnetIpAdapter::new(connection, vec![mapping]).expect("create foreign BACnet adapter");

    let samples = adapter
        .read_telemetry()
        .await
        .expect("read through BACnet foreign device session");
    assert_eq!(samples[0].value, TelemetryValue::Float(21.5));

    let registered = timeout(Duration::from_secs(1), async {
        loop {
            let entry = {
                let mut state = bbmd_state.lock().await;
                state.fdt().first().map(|entry| (entry.ip, entry.ttl))
            };
            if let Some(entry) = entry {
                break entry;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("foreign device registration did not reach BBMD");
    assert_eq!(registered.0, Ipv4Addr::LOCALHOST.octets());
    assert_eq!(registered.1, 120);

    server.await.expect("BACnet/IP BBMD device task");
}

#[tokio::test]
async fn bacnet_ip_cov_emits_only_changed_values_and_keeps_polling_fallback() {
    let transport = BipTransport::new(Ipv4Addr::LOCALHOST, 0, Ipv4Addr::BROADCAST);
    let mut network = NetworkLayer::new(transport);
    let mut receiver = network.start().await.expect("start BACnet/IP COV device");
    let device_mac = network.local_mac().to_vec();
    let device_port = u16::from_be_bytes([device_mac[4], device_mac[5]]);

    let server = tokio::spawn(async move {
        let subscribe_message = timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("COV subscribe timed out")
            .expect("BACnet receive channel closed");
        let Apdu::ConfirmedRequest(subscribe_apdu) =
            apdu::decode_apdu(subscribe_message.apdu.clone()).expect("decode SubscribeCOV")
        else {
            panic!("expected confirmed SubscribeCOV request");
        };
        assert_eq!(
            subscribe_apdu.service_choice,
            ConfirmedServiceChoice::SUBSCRIBE_COV
        );
        let subscription = SubscribeCOVRequest::decode(&subscribe_apdu.service_request)
            .expect("decode SubscribeCOV payload");
        assert_eq!(subscription.lifetime, Some(300));
        assert_eq!(subscription.issue_confirmed_notifications, Some(false));

        let mut ack = BytesMut::new();
        encode_apdu(
            &mut ack,
            &Apdu::SimpleAck(SimpleAck {
                invoke_id: subscribe_apdu.invoke_id,
                service_choice: ConfirmedServiceChoice::SUBSCRIBE_COV,
            }),
        )
        .expect("encode SubscribeCOV ack");
        network
            .send_apdu(
                &ack,
                &subscribe_message.source_mac,
                false,
                NetworkPriority::NORMAL,
            )
            .await
            .expect("send SubscribeCOV ack");

        let rpm_message = timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("initial RPM timed out")
            .expect("BACnet receive channel closed");
        let Apdu::ConfirmedRequest(rpm_apdu) =
            apdu::decode_apdu(rpm_message.apdu.clone()).expect("decode initial RPM")
        else {
            panic!("expected confirmed RPM request");
        };
        assert_eq!(
            rpm_apdu.service_choice,
            ConfirmedServiceChoice::READ_PROPERTY_MULTIPLE
        );
        let rpm = ReadPropertyMultipleRequest::decode(&rpm_apdu.service_request)
            .expect("decode initial RPM payload");
        let rpm_ack = ReadPropertyMultipleACK {
            list_of_read_access_results: rpm
                .list_of_read_access_specs
                .into_iter()
                .map(|spec| ReadAccessResult {
                    object_identifier: spec.object_identifier,
                    list_of_results: spec
                        .list_of_property_references
                        .into_iter()
                        .map(|property| ReadResultElement {
                            property_identifier: property.property_identifier,
                            property_array_index: property.property_array_index,
                            property_value: Some(real_value(20.0)),
                            error: None,
                        })
                        .collect(),
                })
                .collect(),
        };
        let mut rpm_service = BytesMut::new();
        rpm_ack.encode(&mut rpm_service);
        let mut rpm_response = BytesMut::new();
        encode_apdu(
            &mut rpm_response,
            &Apdu::ComplexAck(ComplexAck {
                segmented: false,
                more_follows: false,
                invoke_id: rpm_apdu.invoke_id,
                sequence_number: None,
                proposed_window_size: None,
                service_choice: ConfirmedServiceChoice::READ_PROPERTY_MULTIPLE,
                service_ack: rpm_service.freeze(),
            }),
        )
        .expect("encode RPM response");
        network
            .send_apdu(
                &rpm_response,
                &rpm_message.source_mac,
                false,
                NetworkPriority::NORMAL,
            )
            .await
            .expect("send RPM response");

        let notification = COVNotificationRequest {
            subscriber_process_identifier: subscription.subscriber_process_identifier,
            initiating_device_identifier: ObjectIdentifier::new_addressable(ObjectType::DEVICE, 42)
                .expect("device object"),
            monitored_object_identifier: subscription.monitored_object_identifier,
            time_remaining: 299,
            list_of_values: vec![BACnetPropertyValue {
                property_identifier: PropertyIdentifier::PRESENT_VALUE,
                property_array_index: None,
                value: real_value(21.75),
                priority: None,
            }],
        };
        let mut cov_service = BytesMut::new();
        notification.encode(&mut cov_service);
        let mut cov_apdu = BytesMut::new();
        encode_apdu(
            &mut cov_apdu,
            &Apdu::UnconfirmedRequest(UnconfirmedRequest {
                service_choice: UnconfirmedServiceChoice::UNCONFIRMED_COV_NOTIFICATION,
                service_request: cov_service.freeze(),
            }),
        )
        .expect("encode COV notification");
        network
            .send_apdu(
                &cov_apdu,
                &rpm_message.source_mac,
                false,
                NetworkPriority::NORMAL,
            )
            .await
            .expect("send COV notification");
        sleep(Duration::from_millis(20)).await;
        network.stop().await.expect("stop BACnet/IP COV device");
    });

    let settings = BacnetIpConnectionSettings {
        bind_address: Ipv4Addr::LOCALHOST.to_string(),
        apdu_timeout_ms: 1_000,
        apdu_retries: 0,
        cov: Some(BacnetCovSettings {
            lifetime_seconds: 300,
            confirmed_notifications: false,
            fallback_poll_interval_ms: 60_000,
        }),
        ..BacnetIpConnectionSettings::default()
    };
    let connection = ProtocolConnection::bacnet_ip(
        "bacnet-main",
        Some(format!("127.0.0.1:{device_port}")),
        settings,
    );
    let mut adapter = BacnetIpAdapter::new(
        connection,
        vec![point("temperature"), point("temperature_alias")],
    )
    .expect("create BACnet COV adapter");

    let initial = adapter
        .read_telemetry()
        .await
        .expect("initial BACnet snapshot");
    assert_eq!(initial.len(), 2);
    assert!(initial
        .iter()
        .all(|sample| sample.value == TelemetryValue::Float(20.0)));

    server.await.expect("BACnet/IP COV device task");
    sleep(Duration::from_millis(20)).await;
    let changed = adapter
        .read_telemetry()
        .await
        .expect("consume BACnet COV notification");
    assert_eq!(changed.len(), 2);
    assert!(changed
        .iter()
        .all(|sample| sample.value == TelemetryValue::Float(21.75)));
    assert!(adapter
        .read_telemetry()
        .await
        .expect("idle COV cycle")
        .is_empty());

    let metrics = adapter.cov_runtime_metrics();
    assert_eq!(metrics.active_subscriptions, 1);
    assert_eq!(metrics.notifications_received, 1);
    assert_eq!(metrics.subscription_failures, 0);
    assert_eq!(metrics.fallback_polls, 1);
}

#[tokio::test]
async fn bacnet_ip_cov_rejection_falls_back_to_snapshot_without_collection_outage() {
    let transport = BipTransport::new(Ipv4Addr::LOCALHOST, 0, Ipv4Addr::BROADCAST);
    let mut network = NetworkLayer::new(transport);
    let mut receiver = network
        .start()
        .await
        .expect("start BACnet/IP fallback device");
    let device_mac = network.local_mac().to_vec();
    let device_port = u16::from_be_bytes([device_mac[4], device_mac[5]]);

    let server = tokio::spawn(async move {
        let subscribe_message = timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("COV subscribe timed out")
            .expect("BACnet receive channel closed");
        let Apdu::ConfirmedRequest(subscribe_apdu) =
            apdu::decode_apdu(subscribe_message.apdu.clone()).expect("decode SubscribeCOV")
        else {
            panic!("expected confirmed SubscribeCOV request");
        };
        assert_eq!(
            subscribe_apdu.service_choice,
            ConfirmedServiceChoice::SUBSCRIBE_COV
        );
        let mut rejection = BytesMut::new();
        encode_apdu(
            &mut rejection,
            &Apdu::Reject(RejectPdu {
                invoke_id: subscribe_apdu.invoke_id,
                reject_reason: RejectReason::UNRECOGNIZED_SERVICE,
            }),
        )
        .expect("encode COV rejection");
        network
            .send_apdu(
                &rejection,
                &subscribe_message.source_mac,
                false,
                NetworkPriority::NORMAL,
            )
            .await
            .expect("send COV rejection");

        let rpm_message = timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("fallback RPM timed out")
            .expect("BACnet receive channel closed");
        let Apdu::ConfirmedRequest(rpm_apdu) =
            apdu::decode_apdu(rpm_message.apdu.clone()).expect("decode fallback RPM")
        else {
            panic!("expected confirmed fallback RPM request");
        };
        assert_eq!(
            rpm_apdu.service_choice,
            ConfirmedServiceChoice::READ_PROPERTY_MULTIPLE
        );
        let rpm = ReadPropertyMultipleRequest::decode(&rpm_apdu.service_request)
            .expect("decode fallback RPM payload");
        let rpm_ack = ReadPropertyMultipleACK {
            list_of_read_access_results: rpm
                .list_of_read_access_specs
                .into_iter()
                .map(|spec| ReadAccessResult {
                    object_identifier: spec.object_identifier,
                    list_of_results: spec
                        .list_of_property_references
                        .into_iter()
                        .map(|property| ReadResultElement {
                            property_identifier: property.property_identifier,
                            property_array_index: property.property_array_index,
                            property_value: Some(real_value(18.5)),
                            error: None,
                        })
                        .collect(),
                })
                .collect(),
        };
        let mut service = BytesMut::new();
        rpm_ack.encode(&mut service);
        let mut response = BytesMut::new();
        encode_apdu(
            &mut response,
            &Apdu::ComplexAck(ComplexAck {
                segmented: false,
                more_follows: false,
                invoke_id: rpm_apdu.invoke_id,
                sequence_number: None,
                proposed_window_size: None,
                service_choice: ConfirmedServiceChoice::READ_PROPERTY_MULTIPLE,
                service_ack: service.freeze(),
            }),
        )
        .expect("encode fallback RPM response");
        network
            .send_apdu(
                &response,
                &rpm_message.source_mac,
                false,
                NetworkPriority::NORMAL,
            )
            .await
            .expect("send fallback RPM response");
        network
            .stop()
            .await
            .expect("stop BACnet/IP fallback device");
    });

    let settings = BacnetIpConnectionSettings {
        bind_address: Ipv4Addr::LOCALHOST.to_string(),
        apdu_timeout_ms: 1_000,
        apdu_retries: 0,
        cov: Some(BacnetCovSettings {
            lifetime_seconds: 300,
            confirmed_notifications: false,
            fallback_poll_interval_ms: 60_000,
        }),
        ..BacnetIpConnectionSettings::default()
    };
    let connection = ProtocolConnection::bacnet_ip(
        "bacnet-main",
        Some(format!("127.0.0.1:{device_port}")),
        settings,
    );
    let mut adapter =
        BacnetIpAdapter::new(connection, vec![point("temperature")]).expect("create adapter");

    let samples = adapter
        .read_telemetry()
        .await
        .expect("fallback snapshot remains available");
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].value, TelemetryValue::Float(18.5));
    let metrics = adapter.cov_runtime_metrics();
    assert_eq!(metrics.active_subscriptions, 0);
    assert_eq!(metrics.subscription_failures, 1);
    assert_eq!(metrics.fallback_polls, 1);

    server.await.expect("BACnet/IP fallback device task");
}
