use crate::{CommandCandidate, DeviceSpec, NumberRange};
use thiserror::Error;

#[derive(Clone, Debug, Default)]
pub struct PolicyEngine;

impl PolicyEngine {
    pub fn validate_command(
        &self,
        spec: &DeviceSpec,
        candidate: &CommandCandidate,
    ) -> Result<(), PolicyViolation> {
        let command =
            spec.command(&candidate.command)
                .ok_or_else(|| PolicyViolation::UnknownCommand {
                    command: candidate.command.clone(),
                })?;

        if command.requires_confirmation && candidate.confirmation_token.is_none() {
            return Err(PolicyViolation::ConfirmationRequired {
                command: command.id.clone(),
            });
        }

        for parameter in &command.parameters {
            let value = candidate.parameters.get(&parameter.id).ok_or_else(|| {
                PolicyViolation::MissingParameter {
                    parameter: parameter.id.clone(),
                }
            })?;

            if let Some(range) = parameter.range {
                let Some(number) = value.as_f64() else {
                    return Err(PolicyViolation::ParameterNotNumeric {
                        parameter: parameter.id.clone(),
                    });
                };

                if !range.contains(number) {
                    return Err(PolicyViolation::ParameterOutOfRange {
                        parameter: parameter.id.clone(),
                        value: number,
                        range,
                    });
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum PolicyViolation {
    #[error("unknown command `{command}`")]
    UnknownCommand { command: String },
    #[error("missing required parameter `{parameter}`")]
    MissingParameter { parameter: String },
    #[error("parameter `{parameter}` is not numeric")]
    ParameterNotNumeric { parameter: String },
    #[error("parameter `{parameter}` value {value} is outside range {min}..={max}", min = .range.min, max = .range.max)]
    ParameterOutOfRange {
        parameter: String,
        value: f64,
        range: NumberRange,
    },
    #[error("command `{command}` requires human confirmation")]
    ConfirmationRequired { command: String },
}
