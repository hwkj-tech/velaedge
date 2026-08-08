use std::collections::{BTreeMap, BTreeSet};

use edge_core::{
    parse_iec104_point_address, parse_opc_ua_browse_path, validate_bacnet_point,
    validate_command_flow, validate_modbus_point_options, validate_omron_fins_point,
    validate_opc_ua_node_id, validate_siemens_s7_point, EdgeConfigPackage, ProtocolType,
    MAX_DATA_CONFIG_RETRY_COUNT, MAX_DATA_CONFIG_TIMEOUT_MS,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationError {
    pub message: String,
}

pub struct ConfigValidator;

impl ConfigValidator {
    pub fn validate_package(package: &EdgeConfigPackage) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let connections = package
            .protocol_connections
            .iter()
            .map(|connection| (connection.connection_id.as_str(), connection))
            .collect::<BTreeMap<_, _>>();
        let devices = package
            .devices
            .iter()
            .map(|device| device.device_id.as_str())
            .collect::<BTreeSet<_>>();
        let mqtt_sinks = package
            .mqtt_uplinks
            .iter()
            .map(|uplink| uplink.sink_id.as_str())
            .collect::<BTreeSet<_>>();
        let algorithms = package
            .algorithms
            .iter()
            .map(|algorithm| algorithm.id.as_str())
            .collect::<BTreeSet<_>>();
        let point_mappings = package
            .point_mappings
            .iter()
            .map(|mapping| (mapping.point_id.as_str(), mapping))
            .collect::<BTreeMap<_, _>>();

        for connection in &package.protocol_connections {
            if let Err(message) = connection.validate() {
                errors.push(ValidationError {
                    message: format!(
                        "protocol connection `{}`: {message}",
                        connection.connection_id
                    ),
                });
            }
        }

        for uplink in &package.mqtt_uplinks {
            if uplink.username.is_some() != uplink.password_env.is_some() {
                errors.push(ValidationError {
                    message: format!(
                        "mqtt sink `{}` username and password environment reference must be configured together",
                        uplink.sink_id
                    ),
                });
            }
            if uplink
                .username
                .as_deref()
                .is_some_and(|value| value.trim().is_empty())
                || uplink
                    .password_env
                    .as_deref()
                    .is_some_and(|value| value.trim().is_empty())
            {
                errors.push(ValidationError {
                    message: format!(
                        "mqtt sink `{}` credential references must not be empty",
                        uplink.sink_id
                    ),
                });
            }
            if uplink
                .tls_ca_path
                .as_deref()
                .is_some_and(|value| value.trim().is_empty())
            {
                errors.push(ValidationError {
                    message: format!(
                        "mqtt sink `{}` TLS CA path must not be empty",
                        uplink.sink_id
                    ),
                });
            }
            if uplink.tls_ca_path.is_some() && !mqtt_broker_uses_tls(&uplink.broker) {
                errors.push(ValidationError {
                    message: format!(
                        "mqtt sink `{}` TLS CA path requires an mqtts:// broker",
                        uplink.sink_id
                    ),
                });
            }
        }

        for mapping in &package.point_mappings {
            if let Err(message) =
                validate_modbus_point_options(&mapping.address, mapping.value_type, mapping.access)
            {
                errors.push(ValidationError {
                    message: format!("point `{}`: {message}", mapping.point_id),
                });
            }
            if let Some(connection) = connections.get(mapping.protocol_connection_id.as_str()) {
                if connection.protocol == ProtocolType::OpcUa {
                    let validation =
                        match mapping.address.kind.as_str() {
                            "node_id" => validate_opc_ua_node_id(&mapping.address.value),
                            "browse_path" => {
                                parse_opc_ua_browse_path(&mapping.address.value).map(|_| ())
                            }
                            _ => Err("OPC UA address kind must be `node_id` or `browse_path`"
                                .to_string()),
                        };
                    if let Err(message) = validation {
                        errors.push(ValidationError {
                            message: format!("point `{}`: {message}", mapping.point_id),
                        });
                    }
                }
                if connection.protocol == ProtocolType::Iec104 {
                    if mapping.address.kind != "iec104_ioa" {
                        errors.push(ValidationError {
                            message: format!(
                                "point `{}` IEC 104 address kind must be `iec104_ioa`",
                                mapping.point_id
                            ),
                        });
                    } else if let Err(message) = parse_iec104_point_address(&mapping.address.value)
                    {
                        errors.push(ValidationError {
                            message: format!("point `{}`: {message}", mapping.point_id),
                        });
                    }
                }
                if connection.protocol == ProtocolType::BacnetIp {
                    if let Err(message) = validate_bacnet_point(
                        &mapping.address,
                        mapping.value_type,
                        mapping.access,
                        mapping.bacnet,
                    ) {
                        errors.push(ValidationError {
                            message: format!("point `{}`: {message}", mapping.point_id),
                        });
                    }
                }
                if connection.protocol == ProtocolType::SiemensS7 {
                    if let Err(message) = validate_siemens_s7_point(
                        &mapping.address,
                        mapping.value_type,
                        mapping.access,
                    ) {
                        errors.push(ValidationError {
                            message: format!("point `{}`: {message}", mapping.point_id),
                        });
                    }
                }
                if connection.protocol == ProtocolType::OmronFins {
                    if let Err(message) = validate_omron_fins_point(
                        &mapping.address,
                        mapping.value_type,
                        mapping.access,
                    ) {
                        errors.push(ValidationError {
                            message: format!("point `{}`: {message}", mapping.point_id),
                        });
                    }
                }
            } else {
                errors.push(ValidationError {
                    message: format!(
                        "point `{}` references missing protocol connection `{}`",
                        mapping.point_id, mapping.protocol_connection_id
                    ),
                });
            }
            if !devices.contains(mapping.device_id.as_str()) {
                errors.push(ValidationError {
                    message: format!(
                        "point `{}` references missing device `{}`",
                        mapping.point_id, mapping.device_id
                    ),
                });
            }
        }

        for task in &package.collection_tasks {
            if task.task_id.trim().is_empty() {
                errors.push(ValidationError {
                    message: "collection task id is required".to_string(),
                });
            }
            if task.device_id.trim().is_empty() {
                errors.push(ValidationError {
                    message: format!("collection task `{}` device id is required", task.task_id),
                });
            } else if !devices.contains(task.device_id.as_str()) {
                errors.push(ValidationError {
                    message: format!(
                        "collection task `{}` references missing device `{}`",
                        task.task_id, task.device_id
                    ),
                });
            }
            if task.interval_ms == 0 {
                errors.push(ValidationError {
                    message: format!(
                        "collection task `{}` interval must be greater than zero",
                        task.task_id
                    ),
                });
            }
            if task.point_ids.is_empty() {
                errors.push(ValidationError {
                    message: format!(
                        "collection task `{}` must contain at least one point",
                        task.task_id
                    ),
                });
            }
            for point_id in &task.point_ids {
                let Some(mapping) = point_mappings.get(point_id.as_str()) else {
                    errors.push(ValidationError {
                        message: format!(
                            "collection task `{}` references missing point `{}`",
                            task.task_id, point_id
                        ),
                    });
                    continue;
                };
                if mapping.device_id != task.device_id {
                    errors.push(ValidationError {
                        message: format!(
                            "collection task `{}` point `{}` belongs to device `{}`, expected `{}`",
                            task.task_id, point_id, mapping.device_id, task.device_id
                        ),
                    });
                }
                if !mapping.access.is_readable() {
                    errors.push(ValidationError {
                        message: format!(
                            "collection task `{}` references write-only point `{}`",
                            task.task_id, point_id
                        ),
                    });
                }
            }
        }

        for data_config in &package.data_configs {
            if data_config.config_id.trim().is_empty() {
                errors.push(ValidationError {
                    message: "data config id is required".to_string(),
                });
            }
            if data_config.device_id.trim().is_empty() {
                errors.push(ValidationError {
                    message: format!(
                        "data config `{}` device id is required",
                        data_config.config_id
                    ),
                });
            } else if !devices.contains(data_config.device_id.as_str()) {
                errors.push(ValidationError {
                    message: format!(
                        "data config `{}` references missing device `{}`",
                        data_config.config_id, data_config.device_id
                    ),
                });
            }
            if data_config.protocol_connection_id.trim().is_empty() {
                errors.push(ValidationError {
                    message: format!(
                        "data config `{}` protocol connection is required",
                        data_config.config_id
                    ),
                });
            } else if !connections.contains_key(data_config.protocol_connection_id.as_str()) {
                errors.push(ValidationError {
                    message: format!(
                        "data config `{}` references missing protocol connection `{}`",
                        data_config.config_id, data_config.protocol_connection_id
                    ),
                });
            }
            if !mqtt_sinks.contains(data_config.publish.sink_id.as_str()) {
                errors.push(ValidationError {
                    message: format!(
                        "data config `{}` references missing mqtt sink `{}`",
                        data_config.config_id, data_config.publish.sink_id
                    ),
                });
            }
            if data_config.collection.period_ms == 0 {
                errors.push(ValidationError {
                    message: format!(
                        "data config `{}` collection period must be greater than zero",
                        data_config.config_id
                    ),
                });
            }
            if data_config.collection.timeout_ms == 0
                || data_config.collection.timeout_ms > MAX_DATA_CONFIG_TIMEOUT_MS
            {
                errors.push(ValidationError {
                    message: format!(
                        "data config `{}` collection timeout must be between 1 and {} ms",
                        data_config.config_id, MAX_DATA_CONFIG_TIMEOUT_MS
                    ),
                });
            }
            if data_config.collection.retry_count > MAX_DATA_CONFIG_RETRY_COUNT {
                errors.push(ValidationError {
                    message: format!(
                        "data config `{}` collection retry count must not exceed {}",
                        data_config.config_id, MAX_DATA_CONFIG_RETRY_COUNT
                    ),
                });
            }
            if data_config.points.is_empty() {
                errors.push(ValidationError {
                    message: format!(
                        "data config `{}` must contain at least one point",
                        data_config.config_id
                    ),
                });
            }
            let mut json_fields = BTreeSet::new();
            for point in &data_config.points {
                if point.point_id.trim().is_empty() {
                    errors.push(ValidationError {
                        message: format!(
                            "data config `{}` contains point with empty id",
                            data_config.config_id
                        ),
                    });
                } else if let Some(mapping) = point_mappings.get(point.point_id.as_str()) {
                    if mapping.device_id != data_config.device_id {
                        errors.push(ValidationError {
                            message: format!(
                                "data config `{}` point `{}` belongs to device `{}`, expected `{}`",
                                data_config.config_id,
                                point.point_id,
                                mapping.device_id,
                                data_config.device_id
                            ),
                        });
                    }
                    if mapping.protocol_connection_id != data_config.protocol_connection_id {
                        errors.push(ValidationError {
                            message: format!(
                                "data config `{}` point `{}` uses protocol connection `{}`, expected `{}`",
                                data_config.config_id,
                                point.point_id,
                                mapping.protocol_connection_id,
                                data_config.protocol_connection_id
                            ),
                        });
                    }
                    if !mapping.access.is_readable() {
                        errors.push(ValidationError {
                            message: format!(
                                "data config `{}` references write-only point `{}`",
                                data_config.config_id, point.point_id
                            ),
                        });
                    }
                } else {
                    errors.push(ValidationError {
                        message: format!(
                            "data config `{}` references missing point `{}`",
                            data_config.config_id, point.point_id
                        ),
                    });
                }
                if point.json_field.trim().is_empty() {
                    errors.push(ValidationError {
                        message: format!(
                            "data config `{}` point `{}` json field is required",
                            data_config.config_id, point.point_id
                        ),
                    });
                } else if !json_fields.insert(point.json_field.as_str()) {
                    errors.push(ValidationError {
                        message: format!(
                            "data config `{}` has duplicate json field `{}`",
                            data_config.config_id, point.json_field
                        ),
                    });
                }
            }
            for algorithm_id in &data_config.algorithm_ids {
                if !algorithms.contains(algorithm_id.as_str()) {
                    errors.push(ValidationError {
                        message: format!(
                            "data config `{}` references missing algorithm `{}`",
                            data_config.config_id, algorithm_id
                        ),
                    });
                }
            }
        }

        for command_flow in &package.command_flows {
            if !command_flow.protocol_connection_id.is_empty()
                && !connections.contains_key(command_flow.protocol_connection_id.as_str())
            {
                errors.push(ValidationError {
                    message: format!(
                        "command flow `{}` references missing protocol connection `{}`",
                        command_flow.flow_id, command_flow.protocol_connection_id
                    ),
                });
            }
            if !mqtt_sinks.contains(command_flow.mqtt_connection_id.as_str()) {
                errors.push(ValidationError {
                    message: format!(
                        "command flow `{}` references missing MQTT connection `{}`",
                        command_flow.flow_id, command_flow.mqtt_connection_id
                    ),
                });
            }
            if let Err(message) = validate_command_flow(command_flow, &package.point_mappings) {
                errors.push(ValidationError { message });
            }
        }

        errors
    }
}

fn mqtt_broker_uses_tls(broker: &str) -> bool {
    matches!(
        broker
            .split_once("://")
            .map(|(scheme, _)| scheme.to_ascii_lowercase())
            .as_deref(),
        Some("mqtts" | "ssl")
    )
}
