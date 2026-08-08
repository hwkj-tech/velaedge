use anyhow::{bail, Result};

const MAX_BIT_READ_QUANTITY: u32 = 2_000;
const MAX_REGISTER_READ_QUANTITY: u32 = 125;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ModbusReadPoint {
    pub mapping_index: usize,
    pub station_id: u8,
    pub function: u8,
    pub offset: u16,
    pub quantity: u16,
    pub is_bit: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ModbusReadWindow {
    pub station_id: u8,
    pub function: u8,
    pub offset: u16,
    pub quantity: u16,
    pub is_bit: bool,
    pub points: Vec<ModbusReadPoint>,
}

pub(crate) fn plan_read_windows(points: Vec<ModbusReadPoint>) -> Result<Vec<ModbusReadWindow>> {
    for point in &points {
        if point.quantity == 0 {
            bail!("Modbus read point quantity must be greater than zero");
        }
        let end = u32::from(point.offset) + u32::from(point.quantity);
        if end > u32::from(u16::MAX) + 1 {
            bail!("Modbus read point exceeds address range");
        }
    }

    let mut groups: Vec<Vec<ModbusReadPoint>> = Vec::new();
    for point in points {
        if let Some(group) = groups.iter_mut().find(|group| {
            group.first().is_some_and(|first| {
                first.station_id == point.station_id
                    && first.function == point.function
                    && first.is_bit == point.is_bit
            })
        }) {
            group.push(point);
        } else {
            groups.push(vec![point]);
        }
    }

    let mut windows: Vec<ModbusReadWindow> = Vec::new();
    for mut group in groups {
        group.sort_by_key(|point| (point.offset, point.mapping_index));
        for point in group {
            let point_end = u32::from(point.offset) + u32::from(point.quantity);
            let can_merge = windows.last().is_some_and(|window| {
                if window.station_id != point.station_id
                    || window.function != point.function
                    || window.is_bit != point.is_bit
                {
                    return false;
                }
                let window_start = u32::from(window.offset);
                let window_end = window_start + u32::from(window.quantity);
                let merged_end = window_end.max(point_end);
                let limit = if point.is_bit {
                    MAX_BIT_READ_QUANTITY
                } else {
                    MAX_REGISTER_READ_QUANTITY
                };
                u32::from(point.offset) <= window_end && merged_end - window_start <= limit
            });

            if can_merge {
                let window = windows.last_mut().expect("read window must exist");
                let merged_end =
                    (u32::from(window.offset) + u32::from(window.quantity)).max(point_end);
                window.quantity = (merged_end - u32::from(window.offset)) as u16;
                window.points.push(point);
            } else {
                windows.push(ModbusReadWindow {
                    station_id: point.station_id,
                    function: point.function,
                    offset: point.offset,
                    quantity: point.quantity,
                    is_bit: point.is_bit,
                    points: vec![point],
                });
            }
        }
    }

    Ok(windows)
}

pub(crate) fn extract_point_payload(
    window: &ModbusReadWindow,
    point: ModbusReadPoint,
    payload: &[u8],
) -> Result<Vec<u8>> {
    let relative_offset = u32::from(point.offset)
        .checked_sub(u32::from(window.offset))
        .ok_or_else(|| anyhow::anyhow!("Modbus point starts before its read window"))?;
    let relative_end = relative_offset + u32::from(point.quantity);
    if relative_end > u32::from(window.quantity) {
        bail!("Modbus point exceeds its read window");
    }

    if window.is_bit {
        let mut point_payload = vec![0_u8; point.quantity.div_ceil(8) as usize];
        for bit in 0..u32::from(point.quantity) {
            let source_bit = relative_offset + bit;
            let source_byte = source_bit as usize / 8;
            if source_byte >= payload.len() {
                bail!("Modbus bit response is shorter than its read window");
            }
            if payload[source_byte] & (1 << (source_bit % 8)) != 0 {
                point_payload[bit as usize / 8] |= 1 << (bit % 8);
            }
        }
        return Ok(point_payload);
    }

    let start = relative_offset as usize * 2;
    let end = relative_end as usize * 2;
    let Some(point_payload) = payload.get(start..end) else {
        bail!("Modbus register response is shorter than its read window");
    };
    Ok(point_payload.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_contiguous_points_and_preserves_mapping_indexes() {
        let windows = plan_read_windows(vec![
            point(2, 3, 4, 2, false),
            point(0, 3, 0, 2, false),
            point(1, 3, 2, 2, false),
        ])
        .unwrap();

        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].offset, 0);
        assert_eq!(windows[0].quantity, 6);
        assert_eq!(
            windows[0]
                .points
                .iter()
                .map(|point| point.mapping_index)
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn separates_gaps_functions_stations_and_protocol_limits() {
        let windows = plan_read_windows(vec![
            point(0, 3, 0, 125, false),
            point(1, 3, 125, 1, false),
            point(2, 3, 200, 1, false),
            ModbusReadPoint {
                station_id: 2,
                ..point(3, 3, 201, 1, false)
            },
            point(4, 4, 0, 1, false),
        ])
        .unwrap();

        assert_eq!(windows.len(), 5);
    }

    #[test]
    fn extracts_register_and_non_byte_aligned_bit_values() {
        let register_window = ModbusReadWindow {
            station_id: 1,
            function: 3,
            offset: 10,
            quantity: 4,
            is_bit: false,
            points: Vec::new(),
        };
        assert_eq!(
            extract_point_payload(
                &register_window,
                point(0, 3, 11, 2, false),
                &[0, 1, 0, 2, 0, 3, 0, 4],
            )
            .unwrap(),
            vec![0, 2, 0, 3]
        );

        let bit_window = ModbusReadWindow {
            station_id: 1,
            function: 1,
            offset: 0,
            quantity: 10,
            is_bit: true,
            points: Vec::new(),
        };
        assert_eq!(
            extract_point_payload(&bit_window, point(0, 1, 8, 1, true), &[0, 1]).unwrap(),
            vec![1]
        );
    }

    fn point(
        mapping_index: usize,
        function: u8,
        offset: u16,
        quantity: u16,
        is_bit: bool,
    ) -> ModbusReadPoint {
        ModbusReadPoint {
            mapping_index,
            station_id: 1,
            function,
            offset,
            quantity,
            is_bit,
        }
    }
}
