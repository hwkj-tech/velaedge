use std::collections::BTreeSet;

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
            for point in &data_config.points {
                if point.point_id.trim().is_empty() {
                    errors.push(ValidationError {
                        message: format!(
                            "data config `{}` contains point with empty id",
                            data_config.config_id
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
                }
            }
        }

        errors
    }
}
