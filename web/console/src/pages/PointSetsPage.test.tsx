import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type {
  Dlt645DataIdentifierTemplateResponse,
  PointSetResponse,
  RuntimeProtocolDescriptor,
} from '../api/types';
import { PointSetsPage } from './PointSetsPage';

vi.mock('../api/client', () => ({
  fetchBacnetIpCatalog: vi.fn().mockResolvedValue({
    objectTypes: [
      { name: '模拟输入', objectType: 'analog_input', rawValue: 0, writable: false },
      { name: '模拟输出', objectType: 'analog_output', rawValue: 1, writable: true },
    ],
    properties: [
      { name: '当前值', property: 'present_value', rawValue: 85 },
      { name: '状态标志', property: 'status_flags', rawValue: 111 },
    ],
  }),
}));

const pointSet: PointSetResponse = {
  createdAt: '2026-07-14T00:00:00Z',
  description: '泵站基础点位',
  name: '泵站基础点位',
  pointSetId: 'pump-standard-points',
  points: [
    {
      access: 'read_only',
      address: { kind: 'holding_register', value: '40001' },
      intervalMs: 1000,
      pointId: 'pressure',
      semanticId: 'pump.pressure',
      unit: 'MPa',
      valueType: 'float32',
    },
  ],
  projectId: 'demo-plant',
  protocol: 'ModbusRtu',
  updatedAt: '2026-07-14T00:00:00Z',
};

const projects = [{ name: '示例工厂', projectId: 'demo-plant' }];

const dlt645DataIdentifiers: Dlt645DataIdentifierTemplateResponse[] = [
  {
    dataIdentifier: '02010100',
    decimalPlaces: 1,
    name: 'A 相电压',
    semanticId: 'electric.voltage.a',
    templateId: 'voltage_a',
    unit: 'V',
    valueBytes: 2,
    valueType: 'Float',
  },
  {
    dataIdentifier: '02020100',
    decimalPlaces: 3,
    name: 'A 相电流',
    semanticId: 'electric.current.a',
    templateId: 'current_a',
    unit: 'A',
    valueBytes: 3,
    valueType: 'Float',
  },
];

describe('PointSetsPage', () => {
  it('uses the Runtime capability catalog for protocol choices', () => {
    const protocolCatalog: RuntimeProtocolDescriptor[] = [
      runtimeProtocol('ModbusRtu', 'Modbus RTU', 'serial'),
      runtimeProtocol('SiemensS7', 'Siemens S7', 'tcp'),
      runtimeProtocol('OmronFins', 'Omron FINS', 'tcp_udp'),
    ];
    renderPage({ pointSets: [], protocolCatalog });

    fireEvent.click(screen.getByRole('button', { name: '新建点位集' }));
    const protocolSelect = within(screen.getByRole('dialog', { name: '新建点位集' }))
      .getByLabelText('协议');

    expect(within(protocolSelect).getByRole('option', { name: 'Siemens S7' })).toBeInTheDocument();
    expect(within(protocolSelect).getByRole('option', { name: 'Omron FINS' })).toBeInTheDocument();
    expect(within(protocolSelect).queryByRole('option', { name: 'OPC UA' })).not.toBeInTheDocument();
  });

  it('builds a canonical Siemens S7 point address from structured fields', async () => {
    const onCreate = vi.fn().mockResolvedValue(pointSet);
    renderPage({ onCreate, pointSets: [] });

    fireEvent.click(screen.getByRole('button', { name: '新建点位集' }));
    const dialog = screen.getByRole('dialog', { name: '新建点位集' });
    fireEvent.change(within(dialog).getByLabelText('点位集 ID'), { target: { value: 's7-drive-points' } });
    fireEvent.change(within(dialog).getByLabelText('点位集名称'), { target: { value: 'S7 驱动点位' } });
    fireEvent.change(within(dialog).getByLabelText('协议'), { target: { value: 'SiemensS7' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 Point ID'), { target: { value: 'speed' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 语义 ID'), { target: { value: 'drive.speed' } });

    expect(within(dialog).getByLabelText('点位 1 地址类型')).toHaveValue('s7_address');
    expect(within(dialog).getByLabelText('点位 1 地址值')).toHaveValue('DB1.REAL0');
    fireEvent.change(within(dialog).getByLabelText('点位 1 S7 DB 编号'), { target: { value: '3' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 S7 数据格式'), { target: { value: 'dint' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 S7 字节偏移'), { target: { value: '6' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 读写权限'), { target: { value: 'read_write' } });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    await waitFor(() => expect(onCreate).toHaveBeenCalledTimes(1));
    expect(onCreate.mock.calls[0][0]).toMatchObject({
      protocol: 'SiemensS7',
      points: [{
        access: 'read_write',
        address: { kind: 's7_address', value: 'DB3.DINT6' },
        valueType: 'int32',
      }],
    });
  });

  it('builds a canonical Omron FINS bit address and enforces its area rules', async () => {
    const onCreate = vi.fn().mockResolvedValue(pointSet);
    renderPage({ onCreate, pointSets: [] });

    fireEvent.click(screen.getByRole('button', { name: '新建点位集' }));
    const dialog = screen.getByRole('dialog', { name: '新建点位集' });
    fireEvent.change(within(dialog).getByLabelText('点位集 ID'), { target: { value: 'fins-machine-points' } });
    fireEvent.change(within(dialog).getByLabelText('点位集名称'), { target: { value: 'FINS 机台点位' } });
    fireEvent.change(within(dialog).getByLabelText('协议'), { target: { value: 'OmronFins' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 Point ID'), { target: { value: 'running' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 语义 ID'), { target: { value: 'machine.running' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 数据类型'), { target: { value: 'bool' } });

    const area = within(dialog).getByLabelText('点位 1 FINS 存储区');
    expect(within(area).getByRole('option', { name: '数据存储区 D/DM' })).toBeDisabled();
    fireEvent.change(area, { target: { value: 'H' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 FINS 字地址'), { target: { value: '7' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 FINS 位号'), { target: { value: '3' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 读写权限'), { target: { value: 'read_write' } });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    await waitFor(() => expect(onCreate).toHaveBeenCalledTimes(1));
    expect(onCreate.mock.calls[0][0]).toMatchObject({
      protocol: 'OmronFins',
      points: [{
        access: 'read_write',
        address: { kind: 'fins_address', value: 'H7.3' },
        valueType: 'bool',
      }],
    });
  });

  it('renders persisted point sets as reusable catalog resources', () => {
    renderPage();

    expect(screen.getByRole('heading', { name: '点位集管理' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '查看点位集 泵站基础点位' })).toBeInTheDocument();
    expect(screen.getByText('示例工厂')).toBeInTheDocument();
    expect(screen.getByText('Modbus RTU')).toBeInTheDocument();
    expect(screen.getByText('1000ms')).toBeInTheDocument();
  });

  it('creates a point set as one resource with multiple points', async () => {
    const onCreate = vi.fn().mockResolvedValue(pointSet);
    renderPage({ onCreate, pointSets: [] });

    fireEvent.click(screen.getByRole('button', { name: '新建点位集' }));
    const dialog = screen.getByRole('dialog', { name: '新建点位集' });
    fireEvent.change(within(dialog).getByLabelText('点位集 ID'), { target: { value: 'meter-points' } });
    fireEvent.change(within(dialog).getByLabelText('点位集名称'), { target: { value: '电表点位' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 Point ID'), { target: { value: 'voltage_a' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 语义 ID'), { target: { value: 'meter.voltage_a' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 地址值'), { target: { value: '40001' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 采集周期(ms)'), { target: { value: '2000' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 字节序'), { target: { value: 'little_endian' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 字序'), { target: { value: 'low_word_first' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 缩放系数'), { target: { value: '0.1' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 偏移量'), { target: { value: '-10' } });
    fireEvent.click(within(dialog).getByRole('button', { name: '添加点位' }));
    fireEvent.change(within(dialog).getByLabelText('点位 2 Point ID'), { target: { value: 'current_a' } });
    fireEvent.change(within(dialog).getByLabelText('点位 2 语义 ID'), { target: { value: 'meter.current_a' } });
    fireEvent.change(within(dialog).getByLabelText('点位 2 地址值'), { target: { value: '40003' } });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    await waitFor(() => expect(onCreate).toHaveBeenCalledTimes(1));
    const request = onCreate.mock.calls[0][0];
    expect(request.pointSetId).toBe('meter-points');
    expect(request.projectId).toBe('demo-plant');
    expect(request.points).toHaveLength(2);
    expect(request.points[0].intervalMs).toBe(2000);
    expect(request.points[0].address.modbus).toEqual({
      byteOrder: 'little_endian',
      encoding: 'f32',
      offset: -10,
      scale: 0.1,
      wordOrder: 'low_word_first',
    });
  });

  it('models a register bit as a read-only Boolean point', async () => {
    const onCreate = vi.fn().mockResolvedValue(pointSet);
    renderPage({ onCreate, pointSets: [] });

    fireEvent.click(screen.getByRole('button', { name: '新建点位集' }));
    const dialog = screen.getByRole('dialog', { name: '新建点位集' });
    fireEvent.change(within(dialog).getByLabelText('点位集 ID'), { target: { value: 'status-bits' } });
    fireEvent.change(within(dialog).getByLabelText('点位集名称'), { target: { value: '状态位' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 Point ID'), { target: { value: 'running' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 语义 ID'), { target: { value: 'pump.running' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 地址值'), { target: { value: '40010' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 数据类型'), { target: { value: 'bool' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 寄存器位'), { target: { value: '5' } });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    await waitFor(() => expect(onCreate).toHaveBeenCalledTimes(1));
    expect(onCreate.mock.calls[0][0].points[0]).toMatchObject({
      access: 'read_only',
      address: {
        modbus: {
          bitIndex: 5,
          byteOrder: 'big_endian',
          offset: 0,
          scale: 1,
          wordOrder: 'high_word_first',
        },
      },
      valueType: 'bool',
    });
  });

  it('saves the whole point set and supports Escape', async () => {
    const onSave = vi.fn().mockResolvedValue(pointSet);
    renderPage({ onSave });

    fireEvent.click(screen.getByRole('button', { name: '修改点位集 泵站基础点位' }));
    const dialog = screen.getByRole('dialog', { name: '编辑点位集' });
    fireEvent.change(within(dialog).getByLabelText('点位 1 采集周期(ms)'), { target: { value: '5000' } });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));
    await waitFor(() => expect(onSave).toHaveBeenCalledWith(
      'pump-standard-points',
      expect.objectContaining({ points: [expect.objectContaining({ intervalMs: 5000 })] }),
    ));

    fireEvent.click(screen.getByRole('button', { name: '修改点位集 泵站基础点位' }));
    expect(screen.getByRole('dialog', { name: '编辑点位集' })).toBeInTheDocument();
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByRole('dialog', { name: '编辑点位集' })).not.toBeInTheDocument();
  });

  it('deletes a complete point set after confirmation', async () => {
    const onDelete = vi.fn().mockResolvedValue(undefined);
    renderPage({ onDelete });

    fireEvent.click(screen.getByRole('button', { name: '删除点位集 泵站基础点位' }));
    const dialog = screen.getByRole('dialog', { name: '删除点位集' });
    fireEvent.click(within(dialog).getByRole('button', { name: '确认删除' }));

    await waitFor(() => expect(onDelete).toHaveBeenCalledWith('pump-standard-points'));
  });

  it('builds a structured custom serial frame instead of requiring raw JSON', async () => {
    const onCreate = vi.fn().mockResolvedValue(pointSet);
    renderPage({ onCreate, pointSets: [] });

    fireEvent.click(screen.getByRole('button', { name: '新建点位集' }));
    const dialog = screen.getByRole('dialog', { name: '新建点位集' });
    fireEvent.change(within(dialog).getByLabelText('点位集 ID'), { target: { value: 'vendor-serial' } });
    fireEvent.change(within(dialog).getByLabelText('点位集名称'), { target: { value: '厂商串口点位' } });
    fireEvent.change(within(dialog).getByLabelText('协议'), { target: { value: 'CustomSerial' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 Point ID'), { target: { value: 'temperature' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 语义 ID'), { target: { value: 'sensor.temperature' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 请求帧 HEX'), { target: { value: '10 02' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 成帧方式'), { target: { value: 'cobs' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 请求校验'), { target: { value: 'crc16_ccitt_false' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 响应校验'), { target: { value: 'sum8' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 取值偏移'), { target: { value: '1' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 缩放'), { target: { value: '0.1' } });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    await waitFor(() => expect(onCreate).toHaveBeenCalledTimes(1));
    const request = onCreate.mock.calls[0][0];
    expect(request.points[0].address.kind).toBe('custom_serial_frame');
    expect(JSON.parse(request.points[0].address.value)).toMatchObject({
      schemaVersion: 2,
      frameEncoding: 'cobs',
      requestChecksum: 'crc16_ccitt_false',
      requestHex: '10 02',
      responseChecksum: 'sum8',
      scale: 0.1,
      valueEncoding: 'u16_be',
      valueOffset: 1,
    });
  });

  it('creates an IEC 104 point set with common address and IOA', async () => {
    const onCreate = vi.fn().mockResolvedValue(pointSet);
    renderPage({ onCreate, pointSets: [] });

    fireEvent.click(screen.getByRole('button', { name: '新建点位集' }));
    const dialog = screen.getByRole('dialog', { name: '新建点位集' });
    fireEvent.change(within(dialog).getByLabelText('点位集 ID'), { target: { value: 'iec104-points' } });
    fireEvent.change(within(dialog).getByLabelText('点位集名称'), { target: { value: 'IEC 104 遥测点位' } });
    fireEvent.change(within(dialog).getByLabelText('协议'), { target: { value: 'Iec104' } });

    expect(within(dialog).getByLabelText('点位 1 地址类型')).toHaveValue('iec104_ioa');
    expect(within(dialog).getByLabelText('点位 1 地址值')).toHaveAttribute('placeholder', '1:1001');
    fireEvent.change(within(dialog).getByLabelText('点位 1 Point ID'), { target: { value: 'line_voltage' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 语义 ID'), { target: { value: 'station.line_voltage' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 地址值'), { target: { value: '1:1001' } });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    await waitFor(() => expect(onCreate).toHaveBeenCalledTimes(1));
    expect(onCreate.mock.calls[0][0]).toMatchObject({
      protocol: 'Iec104',
      points: [{ address: { kind: 'iec104_ioa', value: '1:1001' } }],
    });
  });

  it('configures an IEC 104 writable point with SBO control semantics', async () => {
    const onCreate = vi.fn().mockResolvedValue(pointSet);
    renderPage({ onCreate, pointSets: [] });

    fireEvent.click(screen.getByRole('button', { name: '新建点位集' }));
    const dialog = screen.getByRole('dialog', { name: '新建点位集' });
    fireEvent.change(within(dialog).getByLabelText('点位集 ID'), { target: { value: 'iec104-command-points' } });
    fireEvent.change(within(dialog).getByLabelText('点位集名称'), { target: { value: 'IEC 104 控制点位' } });
    fireEvent.change(within(dialog).getByLabelText('协议'), { target: { value: 'Iec104' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 Point ID'), { target: { value: 'breaker_close' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 语义 ID'), { target: { value: 'breaker.close' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 地址值'), { target: { value: '7:1201' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 数据类型'), { target: { value: 'bool' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 读写权限'), { target: { value: 'read_write' } });

    expect(within(dialog).getByLabelText('点位 1 IEC 104 控制类型')).toHaveValue('C_SC_NA_1');
    fireEvent.click(within(dialog).getByLabelText('点位 1 IEC 104 选择后执行'));
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    await waitFor(() => expect(onCreate).toHaveBeenCalledTimes(1));
    expect(onCreate.mock.calls[0][0]).toMatchObject({
      protocol: 'Iec104',
      points: [{
        access: 'read_write',
        iec104: {
          controlType: 'C_SC_NA_1',
          selectBeforeOperate: true,
        },
      }],
    });
  });

  it('configures an IEC 101 writable point with SBO control semantics', async () => {
    const onCreate = vi.fn().mockResolvedValue(pointSet);
    renderPage({ onCreate, pointSets: [] });

    fireEvent.click(screen.getByRole('button', { name: '新建点位集' }));
    const dialog = screen.getByRole('dialog', { name: '新建点位集' });
    fireEvent.change(within(dialog).getByLabelText('点位集 ID'), { target: { value: 'iec101-command-points' } });
    fireEvent.change(within(dialog).getByLabelText('点位集名称'), { target: { value: 'IEC 101 控制点位' } });
    fireEvent.change(within(dialog).getByLabelText('协议'), { target: { value: 'Iec101' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 Point ID'), { target: { value: 'breaker_close' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 语义 ID'), { target: { value: 'breaker.close' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 地址值'), { target: { value: '1:7:1201' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 数据类型'), { target: { value: 'bool' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 读写权限'), { target: { value: 'read_write' } });

    expect(within(dialog).getByLabelText('点位 1 IEC 101 控制类型')).toHaveValue('C_SC_NA_1');
    fireEvent.click(within(dialog).getByLabelText('点位 1 IEC 101 选择后执行'));
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    await waitFor(() => expect(onCreate).toHaveBeenCalledTimes(1));
    expect(onCreate.mock.calls[0][0]).toMatchObject({
      protocol: 'Iec101',
      points: [{
        access: 'read_write',
        address: { kind: 'iec101_ioa', value: '1:7:1201' },
        iec101: {
          controlType: 'C_SC_NA_1',
          selectBeforeOperate: true,
        },
      }],
    });
  });

  it('builds a structured OPC UA semantic BrowsePath without exposing JSON', async () => {
    const onCreate = vi.fn().mockResolvedValue(pointSet);
    renderPage({ onCreate, pointSets: [] });

    fireEvent.click(screen.getByRole('button', { name: '新建点位集' }));
    const dialog = screen.getByRole('dialog', { name: '新建点位集' });
    fireEvent.change(within(dialog).getByLabelText('点位集 ID'), { target: { value: 'opcua-paths' } });
    fireEvent.change(within(dialog).getByLabelText('点位集名称'), { target: { value: 'OPC UA 语义点位' } });
    fireEvent.change(within(dialog).getByLabelText('协议'), { target: { value: 'OpcUa' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 Point ID'), { target: { value: 'service_level' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 语义 ID'), { target: { value: 'server.service_level' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 地址类型'), { target: { value: 'browse_path' } });

    expect(within(dialog).getByLabelText('点位 1 地址值')).toHaveValue('i=85 → 2:?');
    fireEvent.change(within(dialog).getByLabelText('点位 1 路径段 1 命名空间'), { target: { value: '0' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 路径段 1 名称'), { target: { value: 'Server' } });
    fireEvent.click(within(dialog).getByRole('button', { name: '路径段' }));
    fireEvent.change(within(dialog).getByLabelText('点位 1 路径段 2 命名空间'), { target: { value: '0' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 路径段 2 名称'), { target: { value: 'ServiceLevel' } });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    await waitFor(() => expect(onCreate).toHaveBeenCalledTimes(1));
    const address = onCreate.mock.calls[0][0].points[0].address;
    expect(address.kind).toBe('browse_path');
    expect(JSON.parse(address.value)).toEqual({
      startingNode: 'i=85',
      elements: [
        { namespaceIndex: 0, targetName: 'Server' },
        { namespaceIndex: 0, targetName: 'ServiceLevel' },
      ],
    });
  });

  it('configures the exact OPC UA built-in type for writable command points', async () => {
    const onCreate = vi.fn().mockResolvedValue(pointSet);
    renderPage({ onCreate, pointSets: [] });

    fireEvent.click(screen.getByRole('button', { name: '新建点位集' }));
    const dialog = screen.getByRole('dialog', { name: '新建点位集' });
    fireEvent.change(within(dialog).getByLabelText('点位集 ID'), { target: { value: 'opcua-command-points' } });
    fireEvent.change(within(dialog).getByLabelText('点位集名称'), { target: { value: 'OPC UA 控制点位' } });
    fireEvent.change(within(dialog).getByLabelText('协议'), { target: { value: 'OpcUa' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 Point ID'), { target: { value: 'speed_setpoint' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 语义 ID'), { target: { value: 'pump.speed_setpoint' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 地址值'), { target: { value: 'ns=2;s=Pump/SpeedSetpoint' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 数据类型'), { target: { value: 'int32' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 读写权限'), { target: { value: 'read_write' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 OPC UA 写入类型'), { target: { value: 'UInt16' } });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    await waitFor(() => expect(onCreate).toHaveBeenCalledTimes(1));
    expect(onCreate.mock.calls[0][0]).toMatchObject({
      protocol: 'OpcUa',
      points: [{
        access: 'read_write',
        opcUa: { writeDataType: 'UInt16' },
      }],
    });
  });

  it('builds a DL/T 645 point from the shared data identifier catalog', async () => {
    const onCreate = vi.fn().mockResolvedValue(pointSet);
    renderPage({ onCreate, pointSets: [] });

    fireEvent.click(screen.getByRole('button', { name: '新建点位集' }));
    const dialog = screen.getByRole('dialog', { name: '新建点位集' });
    fireEvent.change(within(dialog).getByLabelText('点位集 ID'), { target: { value: 'meter-points' } });
    fireEvent.change(within(dialog).getByLabelText('点位集名称'), { target: { value: '电表点位' } });
    fireEvent.change(within(dialog).getByLabelText('协议'), { target: { value: 'Dlt645' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 电表通信地址'), {
      target: { value: '123456789012' },
    });
    fireEvent.change(within(dialog).getByLabelText('点位 1 常用数据标识'), {
      target: { value: 'voltage_a' },
    });

    expect(within(dialog).getByLabelText('点位 1 Point ID')).toHaveValue('voltage_a');
    expect(within(dialog).getByLabelText('点位 1 语义 ID')).toHaveValue('electric.voltage.a');
    expect(within(dialog).getByLabelText('点位 1 地址值')).toHaveValue('123456789012:02010100:1');
    expect(within(dialog).getByLabelText('点位 1 数据类型')).toHaveValue('float32');
    expect(within(dialog).getByLabelText('点位 1 读写权限')).toHaveValue('read_only');
    expect(within(dialog).getByLabelText('点位 1 单位')).toHaveValue('V');
    expect(within(dialog).getByLabelText('点位 1 响应值字节数')).toHaveValue(2);
    expect(within(dialog).getByLabelText('点位 1 响应值字节数')).toHaveAttribute('readonly');
    expect(within(dialog).getByText('标准目录固定')).toBeInTheDocument();

    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));
    await waitFor(() => expect(onCreate).toHaveBeenCalledTimes(1));
    expect(onCreate.mock.calls[0][0]).toMatchObject({
      protocol: 'Dlt645',
      points: [{
        access: 'read_only',
        address: { kind: 'dlt645_address', value: '123456789012:02010100:1' },
        pointId: 'voltage_a',
        semanticId: 'electric.voltage.a',
        unit: 'V',
        valueType: 'float32',
      }],
    });
  });

  it('requires and persists the response length contract for a vendor DL/T 645 identifier', async () => {
    const onCreate = vi.fn().mockResolvedValue(pointSet);
    renderPage({ onCreate, pointSets: [] });

    fireEvent.click(screen.getByRole('button', { name: '新建点位集' }));
    const dialog = screen.getByRole('dialog', { name: '新建点位集' });
    fireEvent.change(within(dialog).getByLabelText('点位集 ID'), { target: { value: 'vendor-meter-points' } });
    fireEvent.change(within(dialog).getByLabelText('点位集名称'), { target: { value: '厂商电表点位' } });
    fireEvent.change(within(dialog).getByLabelText('协议'), { target: { value: 'Dlt645' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 Point ID'), { target: { value: 'vendor_energy' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 语义 ID'), { target: { value: 'vendor.energy' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 电表通信地址'), { target: { value: '123456789012' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 数据标识 DI'), { target: { value: 'F0010203' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 小数位'), { target: { value: '2' } });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    expect(within(dialog).getByText('第 1 个 DL/T 645 厂商数据标识必须填写 1-251 的响应值字节数')).toBeInTheDocument();
    expect(onCreate).not.toHaveBeenCalled();

    fireEvent.change(within(dialog).getByLabelText('点位 1 响应值字节数'), { target: { value: '4' } });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    await waitFor(() => expect(onCreate).toHaveBeenCalledTimes(1));
    expect(onCreate.mock.calls[0][0]).toMatchObject({
      protocol: 'Dlt645',
      points: [{
        address: { kind: 'dlt645_address', value: '123456789012:F0010203:2:4' },
        pointId: 'vendor_energy',
        semanticId: 'vendor.energy',
      }],
    });
  });

  it('builds a canonical BACnet/IP object-property address from structured fields', async () => {
    const onCreate = vi.fn().mockResolvedValue(pointSet);
    renderPage({ onCreate, pointSets: [] });

    fireEvent.click(screen.getByRole('button', { name: '新建点位集' }));
    const dialog = screen.getByRole('dialog', { name: '新建点位集' });
    fireEvent.change(within(dialog).getByLabelText('点位集 ID'), {
      target: { value: 'ahu-bacnet-points' },
    });
    fireEvent.change(within(dialog).getByLabelText('点位集名称'), {
      target: { value: '空调 BACnet 点位' },
    });
    fireEvent.change(within(dialog).getByLabelText('协议'), {
      target: { value: 'BacnetIp' },
    });

    await waitFor(() => {
      expect(within(dialog).getByLabelText('点位 1 BACnet 对象类型')).toHaveTextContent(
        '模拟输出',
      );
    });
    fireEvent.change(within(dialog).getByLabelText('点位 1 Point ID'), {
      target: { value: 'supply_temperature_setpoint' },
    });
    fireEvent.change(within(dialog).getByLabelText('点位 1 语义 ID'), {
      target: { value: 'ahu.supply_temperature_setpoint' },
    });
    fireEvent.change(within(dialog).getByLabelText('点位 1 BACnet 设备实例号'), {
      target: { value: '1001' },
    });
    fireEvent.change(within(dialog).getByLabelText('点位 1 BACnet 对象类型'), {
      target: { value: 'analog_output' },
    });
    fireEvent.change(within(dialog).getByLabelText('点位 1 BACnet 对象实例号'), {
      target: { value: '7' },
    });
    fireEvent.change(within(dialog).getByLabelText('点位 1 读写权限'), {
      target: { value: 'read_write' },
    });
    fireEvent.change(within(dialog).getByLabelText('点位 1 BACnet 写入优先级'), {
      target: { value: '8' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    await waitFor(() => expect(onCreate).toHaveBeenCalledTimes(1));
    expect(onCreate.mock.calls[0][0]).toMatchObject({
      protocol: 'BacnetIp',
      points: [{
        access: 'read_write',
        bacnet: { writePriority: 8 },
        address: {
          kind: 'bacnet_object_property',
          value: '1001:analog_output:7:present_value',
        },
      }],
    });
  });
});

function renderPage(overrides: Partial<Parameters<typeof PointSetsPage>[0]> = {}) {
  return render(
    <PointSetsPage
      dlt645DataIdentifiers={dlt645DataIdentifiers}
      onCreate={vi.fn().mockResolvedValue(pointSet)}
      onDelete={vi.fn().mockResolvedValue(undefined)}
      onSave={vi.fn().mockResolvedValue(pointSet)}
      pointSets={[pointSet]}
      projects={projects}
      {...overrides}
    />,
  );
}

function runtimeProtocol(
  protocolType: string,
  displayName: string,
  transport: RuntimeProtocolDescriptor['transport'],
): RuntimeProtocolDescriptor {
  return {
    automaticDiscovery: false,
    capabilityId: protocolType.toLowerCase(),
    commandWrite: true,
    displayName,
    maturity: 'deployment_candidate',
    protocolType,
    telemetryRead: true,
    transport,
  };
}
