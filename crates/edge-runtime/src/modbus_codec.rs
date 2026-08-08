use anyhow::{bail, Context, Result};
use edge_core::{
    ModbusByteOrder, ModbusPointOptions, ModbusRegisterEncoding, ModbusWordOrder, PointAddress,
    TelemetryType, TelemetryValue,
};

pub(crate) fn modbus_quantity(
    is_bit_function: bool,
    value_type: TelemetryType,
    address: &PointAddress,
) -> Result<u16> {
    if is_bit_function {
        if value_type != TelemetryType::Boolean {
            bail!("Modbus bit points must use boolean telemetry type");
        }
        if address.modbus.is_some() {
            bail!("Modbus coil and discrete input points cannot define register decoding options");
        }
        return Ok(1);
    }
    if address
        .modbus
        .as_ref()
        .is_some_and(|options| options.bit_index.is_some())
    {
        return Ok(1);
    }
    Ok(effective_encoding(value_type, address)?.register_count())
}

pub(crate) fn decode_modbus_value(
    is_bit_function: bool,
    value_type: TelemetryType,
    address: &PointAddress,
    payload: &[u8],
) -> Result<TelemetryValue> {
    if is_bit_function {
        let byte = payload
            .first()
            .context("Modbus bit response payload is empty")?;
        return Ok(TelemetryValue::Boolean(byte & 1 != 0));
    }
    if !payload.len().is_multiple_of(2) || payload.is_empty() {
        bail!("Modbus register response must contain complete registers");
    }
    let options = address.modbus.clone().unwrap_or_default();
    let mut registers = payload
        .chunks_exact(2)
        .map(|chunk| match options.byte_order {
            ModbusByteOrder::BigEndian => u16::from_be_bytes([chunk[0], chunk[1]]),
            ModbusByteOrder::LittleEndian => u16::from_le_bytes([chunk[0], chunk[1]]),
        })
        .collect::<Vec<_>>();

    if let Some(bit_index) = options.bit_index {
        let register = *registers
            .first()
            .context("Modbus bit field register is missing")?;
        return Ok(TelemetryValue::Boolean(
            register & (1_u16 << bit_index) != 0,
        ));
    }
    if options.word_order == ModbusWordOrder::LowWordFirst {
        registers.reverse();
    }

    match value_type {
        TelemetryType::Float => {
            let raw = decode_float(effective_encoding(value_type, address)?, &registers)?;
            let value = apply_transform(raw, &options)?;
            Ok(TelemetryValue::Float(value))
        }
        TelemetryType::Integer => {
            let raw = decode_integer(effective_encoding(value_type, address)?, &registers)? as f64;
            let value = apply_transform(raw, &options)?;
            if value.fract() != 0.0 {
                bail!(
                    "Modbus integer transform produced a fractional value; configure the point as float"
                );
            }
            if !(i64::MIN as f64..=i64::MAX as f64).contains(&value) {
                bail!("Modbus integer transform exceeds i64 range");
            }
            Ok(TelemetryValue::Integer(value as i64))
        }
        TelemetryType::Boolean => Ok(TelemetryValue::Boolean(registers[0] != 0)),
        TelemetryType::Text => Ok(TelemetryValue::Text(registers[0].to_string())),
    }
}

pub(crate) fn encode_modbus_register_values(
    value_type: TelemetryType,
    address: &PointAddress,
    value: &TelemetryValue,
) -> Result<Vec<u16>> {
    let options = address.modbus.clone().unwrap_or_default();
    if options.bit_index.is_some() {
        bail!("Modbus register bit writes require an atomic mask-write operation");
    }
    let encoding = effective_encoding(value_type, address)?;
    let mut bytes = match (value_type, value) {
        (TelemetryType::Float, TelemetryValue::Float(value)) => {
            let raw = remove_transform(*value, &options)?;
            match encoding {
                ModbusRegisterEncoding::F32 => {
                    let value = raw as f32;
                    if !value.is_finite() {
                        bail!("Modbus f32 write value is outside the finite f32 range");
                    }
                    value.to_be_bytes().to_vec()
                }
                ModbusRegisterEncoding::F64 => raw.to_be_bytes().to_vec(),
                _ => bail!("float Modbus points require f32 or f64 register encoding"),
            }
        }
        (TelemetryType::Integer, TelemetryValue::Integer(value)) => {
            let raw = remove_transform(*value as f64, &options)?;
            encode_integer(encoding, raw)?
        }
        (TelemetryType::Boolean, TelemetryValue::Boolean(value)) => {
            u16::from(*value).to_be_bytes().to_vec()
        }
        (TelemetryType::Text, _) => bail!("Modbus text writes are not supported"),
        _ => bail!("Modbus write value does not match the configured point type"),
    };

    let mut registers = bytes
        .chunks_exact_mut(2)
        .map(|chunk| {
            let value = u16::from_be_bytes([chunk[0], chunk[1]]);
            match options.byte_order {
                ModbusByteOrder::BigEndian => value,
                ModbusByteOrder::LittleEndian => value.swap_bytes(),
            }
        })
        .collect::<Vec<_>>();
    if options.word_order == ModbusWordOrder::LowWordFirst {
        registers.reverse();
    }
    Ok(registers)
}

fn effective_encoding(
    value_type: TelemetryType,
    address: &PointAddress,
) -> Result<ModbusRegisterEncoding> {
    let encoding = address
        .modbus
        .as_ref()
        .and_then(|options| options.encoding)
        .unwrap_or(match value_type {
            TelemetryType::Float => ModbusRegisterEncoding::F32,
            TelemetryType::Integer | TelemetryType::Boolean | TelemetryType::Text => {
                ModbusRegisterEncoding::U16
            }
        });
    match value_type {
        TelemetryType::Float if !encoding.is_float() => {
            bail!("float Modbus points require f32 or f64 register encoding")
        }
        TelemetryType::Integer if !encoding.is_integer() => {
            bail!("integer Modbus points require an integer register encoding")
        }
        _ => Ok(encoding),
    }
}

fn decode_float(encoding: ModbusRegisterEncoding, registers: &[u16]) -> Result<f64> {
    match encoding {
        ModbusRegisterEncoding::F32 => {
            require_registers(registers, 2)?;
            Ok(f32::from_bits(registers_to_u64(registers, 2)? as u32) as f64)
        }
        ModbusRegisterEncoding::F64 => {
            require_registers(registers, 4)?;
            Ok(f64::from_bits(registers_to_u64(registers, 4)?))
        }
        _ => bail!("configured Modbus encoding is not floating point"),
    }
}

fn decode_integer(encoding: ModbusRegisterEncoding, registers: &[u16]) -> Result<i128> {
    let value = registers_to_u64(registers, encoding.register_count() as usize)?;
    Ok(match encoding {
        ModbusRegisterEncoding::U16 => value as u16 as i128,
        ModbusRegisterEncoding::I16 => value as u16 as i16 as i128,
        ModbusRegisterEncoding::U32 => value as u32 as i128,
        ModbusRegisterEncoding::I32 => value as u32 as i32 as i128,
        ModbusRegisterEncoding::U64 => value as i128,
        ModbusRegisterEncoding::I64 => value as i64 as i128,
        ModbusRegisterEncoding::F32 | ModbusRegisterEncoding::F64 => {
            bail!("configured Modbus encoding is not an integer")
        }
    })
}

fn registers_to_u64(registers: &[u16], count: usize) -> Result<u64> {
    require_registers(registers, count)?;
    Ok(registers[..count].iter().fold(0_u64, |value, register| {
        (value << 16) | u64::from(*register)
    }))
}

fn require_registers(registers: &[u16], count: usize) -> Result<()> {
    if registers.len() < count {
        bail!("configured Modbus encoding requires {count} registers");
    }
    Ok(())
}

fn apply_transform(raw: f64, options: &ModbusPointOptions) -> Result<f64> {
    let value = raw.mul_add(options.scale, options.offset);
    if !value.is_finite() {
        bail!("Modbus scale and offset produced a non-finite value");
    }
    Ok(value)
}

fn remove_transform(value: f64, options: &ModbusPointOptions) -> Result<f64> {
    if !value.is_finite() || !options.scale.is_finite() || options.scale == 0.0 {
        bail!("Modbus write transform requires finite values and a non-zero scale");
    }
    let raw = (value - options.offset) / options.scale;
    if !raw.is_finite() {
        bail!("Modbus write transform produced a non-finite raw value");
    }
    Ok(raw)
}

fn encode_integer(encoding: ModbusRegisterEncoding, value: f64) -> Result<Vec<u8>> {
    if value.fract() != 0.0 {
        bail!("Modbus integer write transform must produce a whole raw value");
    }
    macro_rules! checked {
        ($type:ty) => {{
            if !(<$type>::MIN as f64..=<$type>::MAX as f64).contains(&value) {
                bail!("Modbus integer write value is outside the configured encoding range");
            }
            (value as $type).to_be_bytes().to_vec()
        }};
    }
    Ok(match encoding {
        ModbusRegisterEncoding::U16 => checked!(u16),
        ModbusRegisterEncoding::I16 => checked!(i16),
        ModbusRegisterEncoding::U32 => checked!(u32),
        ModbusRegisterEncoding::I32 => checked!(i32),
        ModbusRegisterEncoding::U64 => checked!(u64),
        ModbusRegisterEncoding::I64 => checked!(i64),
        ModbusRegisterEncoding::F32 | ModbusRegisterEncoding::F64 => {
            bail!("integer Modbus points require an integer register encoding")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(options: ModbusPointOptions) -> PointAddress {
        PointAddress::modbus_holding_register(40001).with_modbus_options(options)
    }

    #[test]
    fn decodes_cdab_f32_with_engineering_transform() {
        let options = ModbusPointOptions {
            encoding: Some(ModbusRegisterEncoding::F32),
            word_order: ModbusWordOrder::LowWordFirst,
            scale: 0.1,
            offset: 2.0,
            ..Default::default()
        };
        let raw = 123.0_f32.to_be_bytes();
        let payload = [raw[2], raw[3], raw[0], raw[1]];
        let decoded =
            decode_modbus_value(false, TelemetryType::Float, &address(options), &payload).unwrap();
        assert_eq!(decoded, TelemetryValue::Float(14.3));
    }

    #[test]
    fn decodes_little_endian_signed_integer_and_register_bit() {
        let integer = address(ModbusPointOptions {
            encoding: Some(ModbusRegisterEncoding::I16),
            byte_order: ModbusByteOrder::LittleEndian,
            ..Default::default()
        });
        assert_eq!(
            decode_modbus_value(false, TelemetryType::Integer, &integer, &[0xFE, 0xFF]).unwrap(),
            TelemetryValue::Integer(-2)
        );

        let bit = address(ModbusPointOptions {
            bit_index: Some(5),
            ..Default::default()
        });
        assert_eq!(
            decode_modbus_value(false, TelemetryType::Boolean, &bit, &[0x00, 0x20]).unwrap(),
            TelemetryValue::Boolean(true)
        );
    }

    #[test]
    fn write_encoding_reverses_engineering_transform_and_word_order() {
        let point = address(ModbusPointOptions {
            encoding: Some(ModbusRegisterEncoding::F32),
            word_order: ModbusWordOrder::LowWordFirst,
            scale: 0.5,
            offset: 10.0,
            ..Default::default()
        });
        let registers = encode_modbus_register_values(
            TelemetryType::Float,
            &point,
            &TelemetryValue::Float(12.0),
        )
        .unwrap();
        let raw = 4.0_f32.to_be_bytes();
        assert_eq!(
            registers,
            vec![
                u16::from_be_bytes([raw[2], raw[3]]),
                u16::from_be_bytes([raw[0], raw[1]])
            ]
        );
    }
}
