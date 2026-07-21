use std::collections::{BTreeMap, BTreeSet};

use edge_core::EdgeConfigPackage;
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
            .map(|connection| connection.connection_id.as_str())
            .collect::<BTreeSet<_>>();
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
            if !connections.contains(mapping.protocol_connection_id.as_str()) {
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
            } else if !connections.contains(data_config.protocol_connection_id.as_str()) {
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
