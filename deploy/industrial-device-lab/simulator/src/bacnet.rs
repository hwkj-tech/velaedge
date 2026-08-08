use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use bacnet_client::client::BACnetClient;
use bacnet_encoding::apdu::{
    decode_apdu, encode_apdu, Apdu, ComplexAck, SimpleAck, UnconfirmedRequest,
};
use bacnet_encoding::primitives::{decode_application_value, encode_property_value};
use bacnet_network::layer::NetworkLayer;
use bacnet_services::common::BACnetPropertyValue;
use bacnet_services::cov::{COVNotificationRequest, SubscribeCOVRequest};
use bacnet_services::rpm::{
    ReadAccessResult, ReadPropertyMultipleACK, ReadPropertyMultipleRequest, ReadResultElement,
};
use bacnet_services::who_is::{IAmRequest, WhoIsRequest};
use bacnet_services::write_property::WritePropertyRequest;
use bacnet_transport::bip::BipTransport;
use bacnet_types::enums::{
    ConfirmedServiceChoice, NetworkPriority, ObjectType, PropertyIdentifier, Segmentation,
    UnconfirmedServiceChoice,
};
use bacnet_types::primitives::{ObjectIdentifier, PropertyValue};
use bytes::{Bytes, BytesMut};
use tokio::time::{interval, sleep, timeout, Instant, MissedTickBehavior};
use tracing::{info, warn};

const VENDOR_ID: u16 = 999;

#[derive(Clone, Copy)]
struct DeviceState {
    pressure: f32,
    setpoint: f32,
}

impl Default for DeviceState {
    fn default() -> Self {
        Self {
            pressure: 21.0,
            setpoint: 18.0,
        }
    }
}

#[derive(Clone)]
struct CovSubscription {
    process_id: u32,
    object: ObjectIdentifier,
    destination_mac: Vec<u8>,
    expires_at: Option<Instant>,
}

pub async fn serve(bind: SocketAddr, device_instance: u32, update_interval_ms: u64) -> Result<()> {
    let SocketAddr::V4(bind) = bind else {
        bail!("BACnet/IP simulator requires an IPv4 bind address");
    };
    let device_identifier = ObjectIdentifier::new_addressable(ObjectType::DEVICE, device_instance)
        .context("invalid BACnet device instance")?;
    let transport = BipTransport::new(*bind.ip(), bind.port(), Ipv4Addr::BROADCAST);
    let mut network = NetworkLayer::new(transport);
    let mut receiver = network
        .start()
        .await
        .context("failed to start BACnet/IP network layer")?;
    let mut state = DeviceState::default();
    let mut subscriptions = Vec::<CovSubscription>::new();
    let update_period = Duration::from_millis(update_interval_ms.max(100));
    let mut ticker = interval(update_period);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let started = Instant::now();
    info!(%bind, device_instance, "BACnet/IP simulated controller is ready");

    loop {
        tokio::select! {
            message = receiver.recv() => {
                let Some(message) = message else {
                    bail!("BACnet/IP network receiver stopped");
                };
                if let Err(error) = handle_message(
                    &network,
                    message.apdu.as_ref(),
                    &message.source_mac,
                    device_identifier,
                    &mut state,
                    &mut subscriptions,
                ).await {
                    warn!(%error, "rejected BACnet/IP request");
                }
            }
            _ = ticker.tick() => {
                let elapsed = started.elapsed().as_secs_f32();
                state.pressure = 21.0 + 1.8 * (elapsed * std::f32::consts::TAU / 12.0).sin();
                subscriptions.retain(|subscription| {
                    subscription.expires_at.is_none_or(|deadline| deadline > Instant::now())
                });
                for subscription in subscriptions.clone() {
                    if let Err(error) = send_cov_notification(
                        &network,
                        &subscription,
                        device_identifier,
                        state,
                    ).await {
                        warn!(%error, "failed to send BACnet COV notification");
                    }
                }
            }
            result = tokio::signal::ctrl_c() => {
                result.context("failed to wait for shutdown signal")?;
                break;
            }
        }
    }

    network
        .stop()
        .await
        .context("failed to stop BACnet/IP network layer")?;
    Ok(())
}

async fn handle_message(
    network: &NetworkLayer<BipTransport>,
    bytes: &[u8],
    source_mac: &[u8],
    device_identifier: ObjectIdentifier,
    state: &mut DeviceState,
    subscriptions: &mut Vec<CovSubscription>,
) -> Result<()> {
    match decode_apdu(Bytes::copy_from_slice(bytes)).context("decode BACnet APDU")? {
        Apdu::ConfirmedRequest(request) => match request.service_choice {
            ConfirmedServiceChoice::READ_PROPERTY_MULTIPLE => {
                let rpm = ReadPropertyMultipleRequest::decode(&request.service_request)
                    .context("decode ReadPropertyMultiple")?;
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
                                    property_value: Some(
                                        property_value(spec.object_identifier, state)
                                            .unwrap_or(PropertyValue::Null)
                                            .pipe(encode_value)
                                            .unwrap_or_else(|_| vec![0]),
                                    ),
                                    error: None,
                                })
                                .collect(),
                        })
                        .collect(),
                };
                let mut service = BytesMut::new();
                ack.encode(&mut service);
                send_apdu(
                    network,
                    source_mac,
                    Apdu::ComplexAck(ComplexAck {
                        segmented: false,
                        more_follows: false,
                        invoke_id: request.invoke_id,
                        sequence_number: None,
                        proposed_window_size: None,
                        service_choice: ConfirmedServiceChoice::READ_PROPERTY_MULTIPLE,
                        service_ack: service.freeze(),
                    }),
                )
                .await?;
            }
            ConfirmedServiceChoice::WRITE_PROPERTY => {
                let write = WritePropertyRequest::decode(&request.service_request)
                    .context("decode WriteProperty")?;
                apply_write(&write, state)?;
                send_apdu(
                    network,
                    source_mac,
                    Apdu::SimpleAck(SimpleAck {
                        invoke_id: request.invoke_id,
                        service_choice: ConfirmedServiceChoice::WRITE_PROPERTY,
                    }),
                )
                .await?;
            }
            ConfirmedServiceChoice::SUBSCRIBE_COV => {
                let subscription = SubscribeCOVRequest::decode(&request.service_request)
                    .context("decode SubscribeCOV")?;
                subscriptions.retain(|current| {
                    current.process_id != subscription.subscriber_process_identifier
                        || current.object != subscription.monitored_object_identifier
                        || current.destination_mac != source_mac
                });
                if !subscription.is_cancellation() {
                    subscriptions.push(CovSubscription {
                        process_id: subscription.subscriber_process_identifier,
                        object: subscription.monitored_object_identifier,
                        destination_mac: source_mac.to_vec(),
                        expires_at: subscription
                            .lifetime
                            .map(|seconds| Instant::now() + Duration::from_secs(seconds.into())),
                    });
                }
                send_apdu(
                    network,
                    source_mac,
                    Apdu::SimpleAck(SimpleAck {
                        invoke_id: request.invoke_id,
                        service_choice: ConfirmedServiceChoice::SUBSCRIBE_COV,
                    }),
                )
                .await?;
            }
            choice => bail!("unsupported confirmed BACnet service {choice:?}"),
        },
        Apdu::UnconfirmedRequest(request)
            if request.service_choice == UnconfirmedServiceChoice::WHO_IS =>
        {
            let who_is = WhoIsRequest::decode(&request.service_request).context("decode Who-Is")?;
            let instance = device_identifier.instance_number();
            let matches_range = match (who_is.low_limit, who_is.high_limit) {
                (Some(low), Some(high)) => (low..=high).contains(&instance),
                _ => true,
            };
            if matches_range {
                let mut service = BytesMut::new();
                IAmRequest {
                    object_identifier: device_identifier,
                    max_apdu_length: 1_476,
                    segmentation_supported: Segmentation::NONE,
                    vendor_id: VENDOR_ID,
                }
                .encode(&mut service);
                send_apdu(
                    network,
                    source_mac,
                    Apdu::UnconfirmedRequest(UnconfirmedRequest {
                        service_choice: UnconfirmedServiceChoice::I_AM,
                        service_request: service.freeze(),
                    }),
                )
                .await?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn property_value(object: ObjectIdentifier, state: &DeviceState) -> Option<PropertyValue> {
    match (object.object_type(), object.instance_number()) {
        (ObjectType::ANALOG_INPUT, 1) => Some(PropertyValue::Real(state.pressure)),
        (ObjectType::ANALOG_VALUE, 7) => Some(PropertyValue::Real(state.setpoint)),
        _ => None,
    }
}

fn apply_write(write: &WritePropertyRequest, state: &mut DeviceState) -> Result<()> {
    if write.object_identifier.object_type() != ObjectType::ANALOG_VALUE
        || write.object_identifier.instance_number() != 7
        || write.property_identifier != PropertyIdentifier::PRESENT_VALUE
    {
        bail!("only analog-value 7 present-value is writable");
    }
    if let Some(priority) = write.priority {
        if !(1..=16).contains(&priority) {
            bail!("BACnet write priority must be in 1..=16");
        }
    }
    let (value, consumed) = decode_application_value(&write.property_value, 0)
        .context("decode WriteProperty application value")?;
    if consumed != write.property_value.len() {
        bail!("WriteProperty contains trailing application data");
    }
    let PropertyValue::Real(value) = value else {
        bail!("analog-value 7 requires a real value");
    };
    state.setpoint = value;
    Ok(())
}

async fn send_cov_notification(
    network: &NetworkLayer<BipTransport>,
    subscription: &CovSubscription,
    device_identifier: ObjectIdentifier,
    state: DeviceState,
) -> Result<()> {
    let Some(value) = property_value(subscription.object, &state) else {
        return Ok(());
    };
    let notification = COVNotificationRequest {
        subscriber_process_identifier: subscription.process_id,
        initiating_device_identifier: device_identifier,
        monitored_object_identifier: subscription.object,
        time_remaining: subscription
            .expires_at
            .map(|deadline| deadline.saturating_duration_since(Instant::now()).as_secs() as u32)
            .unwrap_or(0),
        list_of_values: vec![BACnetPropertyValue {
            property_identifier: PropertyIdentifier::PRESENT_VALUE,
            property_array_index: None,
            value: encode_value(value)?,
            priority: None,
        }],
    };
    let mut service = BytesMut::new();
    notification.encode(&mut service);
    send_apdu(
        network,
        &subscription.destination_mac,
        Apdu::UnconfirmedRequest(UnconfirmedRequest {
            service_choice: UnconfirmedServiceChoice::UNCONFIRMED_COV_NOTIFICATION,
            service_request: service.freeze(),
        }),
    )
    .await
}

fn encode_value(value: PropertyValue) -> Result<Vec<u8>> {
    let mut encoded = BytesMut::new();
    encode_property_value(&mut encoded, &value).context("encode BACnet property value")?;
    Ok(encoded.to_vec())
}

async fn send_apdu(
    network: &NetworkLayer<BipTransport>,
    destination_mac: &[u8],
    apdu: Apdu,
) -> Result<()> {
    let mut encoded = BytesMut::new();
    encode_apdu(&mut encoded, &apdu).context("encode BACnet APDU")?;
    network
        .send_apdu(&encoded, destination_mac, false, NetworkPriority::NORMAL)
        .await
        .context("send BACnet APDU")?;
    Ok(())
}

pub async fn check(address: SocketAddr, device_instance: u32, timeout_ms: u64) -> Result<()> {
    let SocketAddr::V4(address) = address else {
        bail!("BACnet/IP readiness check requires an IPv4 address");
    };
    let mut client = BACnetClient::bip_builder()
        .interface(Ipv4Addr::LOCALHOST)
        .port(0)
        .apdu_timeout_ms(timeout_ms)
        .build()
        .await
        .context("start BACnet readiness client")?;
    let mut mac = address.ip().octets().to_vec();
    mac.extend_from_slice(&address.port().to_be_bytes());
    client
        .who_is_directed(&mac, Some(device_instance), Some(device_instance))
        .await
        .context("send directed Who-Is")?;
    timeout(Duration::from_millis(timeout_ms), async {
        loop {
            if client.get_device(device_instance).await.is_some() {
                return;
            }
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .context("BACnet readiness discovery timed out")?;
    client
        .stop()
        .await
        .context("stop BACnet readiness client")?;
    Ok(())
}

trait Pipe: Sized {
    fn pipe<T>(self, apply: impl FnOnce(Self) -> T) -> T {
        apply(self)
    }
}

impl<T> Pipe for T {}
