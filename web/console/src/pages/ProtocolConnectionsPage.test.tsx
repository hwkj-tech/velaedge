import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { ProtocolConnectionsPage } from './ProtocolConnectionsPage';

const circuitBreakerFixture = {
  enabled: true,
  failureThreshold: 5,
  openDurationMs: 30_000,
  halfOpenSuccessThreshold: 1,
};

const opcUaDefaultsFixture = {
  securityPolicy: 'none' as const,
  messageSecurityMode: 'none' as const,
  authMode: 'anonymous' as const,
  username: null,
  passwordEnv: null,
  userCertificatePath: null,
  userPrivateKeyPath: null,
  pkiDir: './data/opcua-pki',
  trustServerCerts: false,
  verifyServerCerts: true,
  connectTimeoutMs: 5_000,
  requestTimeoutMs: 5_000,
  sessionTimeoutMs: 60_000,
  sessionRetryLimit: 3,
};

const connectionFixture = {
  edgeId: 'edge-dev', connectionId: 'modbus-line-a', protocol: 'Modbus TCP',
  protocolType: 'ModbusTcp' as const, endpoint: '10.12.0.20:502', status: '启用',
  policy: '1000ms timeout / 3 retry',
  circuitBreaker: circuitBreakerFixture,
};

describe('ProtocolConnectionsPage', () => {
  it('shows protocol connection table and editor fields', () => {
    render(<ProtocolConnectionsPage connections={[connectionFixture]} selectedEdgeId="edge-dev" />);

    expect(screen.getByText('连接清单')).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: '编辑连接 modbus-line-a' }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '编辑连接 modbus-line-a' }));
    expect(screen.getByText('编辑连接 modbus-line-a')).toBeInTheDocument();
    expect(screen.getByLabelText('协议类型')).toBeInTheDocument();
  });

  it('shows an explicit row action for editing protocol connections', () => {
    render(<ProtocolConnectionsPage connections={[connectionFixture]} selectedEdgeId="edge-dev" />);

    expect(screen.getByRole('columnheader', { name: '操作' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '修改连接 modbus-line-a' }));

    expect(screen.getByRole('dialog', { name: '编辑连接 modbus-line-a' })).toBeInTheDocument();
    expect(screen.getByLabelText('端点')).toHaveValue('10.12.0.20:502');
  });

  it('saves edited protocol connection drafts from the editor drawer', async () => {
    const onSaveConnection = vi.fn().mockResolvedValue(undefined);

    render(
      <ProtocolConnectionsPage
        connections={[connectionFixture]}
        selectedEdgeId="edge-dev"
        onSaveConnection={onSaveConnection}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '编辑连接 modbus-line-a' }));
    fireEvent.change(screen.getByLabelText('协议类型'), {
      target: { value: 'OpcUa' },
    });
    fireEvent.change(screen.getByLabelText('端点'), {
      target: { value: 'opc.tcp://10.12.0.80:4840' },
    });
    fireEvent.click(
      within(screen.getByRole('dialog', { name: '编辑连接 modbus-line-a' })).getByRole(
        'button',
        { name: '保存' },
      ),
    );

    await waitFor(() => {
      expect(onSaveConnection).toHaveBeenCalledWith(
        'edge-dev',
        'modbus-line-a',
        {
          endpoint: 'opc.tcp://10.12.0.80:4840',
          protocolType: 'OpcUa',
          opcUa: opcUaDefaultsFixture,
          circuitBreaker: circuitBreakerFixture,
        },
      );
    });
    expect(screen.getByText('已保存')).toBeInTheDocument();
  });

  it('does not render edge selection context inside the page toolbar', () => {
    render(
      <ProtocolConnectionsPage
        edges={[
          {
            edgeId: 'edge-dev',
            displayName: '研发实验室边端',
            site: '研发/实验室',
            runtimeId: 'runtime-dev',
            status: '健康',
            resources: '18% / 42% / 61%',
            heartbeat: '8 秒前',
            capabilities: ['protocol:modbus-tcp'],
          },
          {
            edgeId: 'edge-prod',
            displayName: '产线边端',
            site: '制造/一线',
            runtimeId: 'runtime-prod',
            status: '健康',
            resources: '22% / 48% / 66%',
            heartbeat: '6 秒前',
            capabilities: ['protocol:opcua'],
          },
        ]}
        selectedEdgeId="edge-dev"
      />,
    );

    expect(screen.queryByLabelText('当前边端')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('配置边端')).not.toBeInTheDocument();
  });

  it('hides the edge selector in sidebar list mode', () => {
    render(<ProtocolConnectionsPage mode="list" selectedEdgeId="edge-dev" />);

    expect(screen.getByText('连接清单')).toBeInTheDocument();
    expect(screen.queryByLabelText('查看边端')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('配置边端')).not.toBeInTheDocument();
  });

  it('runs protocol validation through the backend handler', async () => {
    const onValidateConnection = vi.fn().mockResolvedValue({
      action: 'validate_config',
      details: ['协议连接 1 个'],
      message: '配置校验已完成',
      status: '已通过',
    });

    render(
      <ProtocolConnectionsPage
        selectedEdgeId="edge-dev"
        onValidateConnection={onValidateConnection}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '校验连接' }));
    await waitFor(() => {
      expect(onValidateConnection).toHaveBeenCalledWith('edge-dev');
    });
    expect(await screen.findByText('连接校验 已通过')).toBeInTheDocument();
  });

  it('opens a dialog before creating protocol connections', () => {
    render(<ProtocolConnectionsPage selectedEdgeId="edge-dev" />);

    fireEvent.click(screen.getByRole('button', { name: '新建连接' }));
    expect(screen.getByRole('dialog', { name: '新建协议连接' })).toBeInTheDocument();
    expect(screen.getByLabelText('新建协议类型')).toBeInTheDocument();
    expect(screen.getByLabelText('新建串口设备')).toBeInTheDocument();
    expect(screen.getByLabelText('新建波特率')).toHaveValue(9600);
    expect(screen.getByLabelText('新建校验位')).toHaveValue('none');
  });

  it('creates and selects a new protocol connection from dialog fields', async () => {
    const onCreateConnection = vi.fn().mockResolvedValue({
      edgeId: 'edge-dev',
      connectionId: 'connection-draft-2',
      protocol: 'Modbus TCP',
      protocolType: 'ModbusTcp',
      endpoint: 'runtime://pending',
      status: '启用',
      policy: '1000ms timeout / 3 retry',
      circuitBreaker: circuitBreakerFixture,
    });

    render(
      <ProtocolConnectionsPage
        selectedEdgeId="edge-dev"
        connections={[
          {
            edgeId: 'edge-dev',
            connectionId: 'modbus-line-a',
            protocol: 'Modbus TCP',
            protocolType: 'ModbusTcp',
            endpoint: '10.12.0.20:502',
            status: '启用',
            policy: '1000ms timeout / 3 retry',
            circuitBreaker: circuitBreakerFixture,
          },
          {
            edgeId: 'edge-dev',
            connectionId: 'connection-draft-2',
            protocol: 'Modbus TCP',
            protocolType: 'ModbusTcp',
            endpoint: 'runtime://pending',
            status: '启用',
            policy: '1000ms timeout / 3 retry',
            circuitBreaker: circuitBreakerFixture,
          },
        ]}
        onCreateConnection={onCreateConnection}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '新建连接' }));
    fireEvent.change(screen.getByLabelText('新建协议类型'), {
      target: { value: 'ModbusRtu' },
    });
    fireEvent.change(screen.getByLabelText('新建串口设备'), {
      target: { value: '/dev/ttyUSB1' },
    });
    fireEvent.change(screen.getByLabelText('新建波特率'), {
      target: { value: '19200' },
    });
    fireEvent.click(
      within(screen.getByRole('dialog', { name: '新建协议连接' })).getByRole(
        'button',
        { name: '保存' },
      ),
    );

    await waitFor(() => {
      expect(onCreateConnection).toHaveBeenCalledWith('edge-dev', {
        endpoint: '/dev/ttyUSB1',
        protocolType: 'ModbusRtu',
        serial: {
          baudRate: 19200,
          dataBits: 8,
          parity: 'none',
          port: '/dev/ttyUSB1',
          stopBits: 1,
        },
        opcUa: null,
        circuitBreaker: circuitBreakerFixture,
      });
    });
    expect(screen.getByText('已创建连接 connection-draft-2')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '编辑连接 connection-draft-2' }));
    expect(screen.getByText('编辑连接 connection-draft-2')).toBeInTheDocument();
  });

  it('creates an IEC 104 connection with the standard TCP endpoint', async () => {
    const onCreateConnection = vi.fn().mockResolvedValue({
      edgeId: 'edge-dev',
      connectionId: 'iec104-station-a',
      protocol: 'IEC-104',
      protocolType: 'Iec104',
      endpoint: '127.0.0.1:2404',
      status: '启用',
      policy: 'IEC 104 TCP',
      circuitBreaker: circuitBreakerFixture,
    });
    render(
      <ProtocolConnectionsPage
        selectedEdgeId="edge-dev"
        onCreateConnection={onCreateConnection}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '新建连接' }));
    const dialog = screen.getByRole('dialog', { name: '新建协议连接' });
    fireEvent.change(within(dialog).getByLabelText('新建协议类型'), {
      target: { value: 'Iec104' },
    });

    expect(within(dialog).getByLabelText('新建端点')).toHaveValue('127.0.0.1:2404');
    expect(within(dialog).queryByLabelText('新建串口设备')).not.toBeInTheDocument();
    fireEvent.change(within(dialog).getByLabelText('新建CP56 时区偏移'), {
      target: { value: '480' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    await waitFor(() => expect(onCreateConnection).toHaveBeenCalledWith('edge-dev', {
      endpoint: '127.0.0.1:2404',
      iec104: { cp56TimeZoneOffsetMinutes: 480 },
      protocolType: 'Iec104',
      opcUa: null,
      serial: null,
      circuitBreaker: circuitBreakerFixture,
    }));
  });

  it('edits device-level circuit breaker protection', async () => {
    const onSaveConnection = vi.fn().mockResolvedValue(undefined);
    render(
      <ProtocolConnectionsPage
        connections={[connectionFixture]}
        selectedEdgeId="edge-dev"
        onSaveConnection={onSaveConnection}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '修改连接 modbus-line-a' }));
    fireEvent.change(screen.getByLabelText('连续失败阈值'), {
      target: { value: '3' },
    });
    fireEvent.change(screen.getByLabelText('冷却时间'), {
      target: { value: '15' },
    });
    fireEvent.click(
      within(screen.getByRole('dialog', { name: '编辑连接 modbus-line-a' })).getByRole(
        'button',
        { name: '保存' },
      ),
    );

    await waitFor(() => {
      expect(onSaveConnection).toHaveBeenCalledWith(
        'edge-dev',
        'modbus-line-a',
        expect.objectContaining({
          circuitBreaker: {
            enabled: true,
            failureThreshold: 3,
            openDurationMs: 15_000,
            halfOpenSuccessThreshold: 1,
          },
        }),
      );
    });
  });

  it('creates IEC 101 with serial defaults and station timezone', async () => {
    const onCreateConnection = vi.fn().mockResolvedValue({
      ...connectionFixture,
      connectionId: 'iec101-station-a',
      protocol: 'IEC-101',
      protocolType: 'Iec101',
      endpoint: '/dev/ttyUSB0',
    });
    render(
      <ProtocolConnectionsPage
        selectedEdgeId="edge-dev"
        onCreateConnection={onCreateConnection}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '新建连接' }));
    const dialog = screen.getByRole('dialog', { name: '新建协议连接' });
    fireEvent.change(within(dialog).getByLabelText('新建协议类型'), {
      target: { value: 'Iec101' },
    });

    expect(within(dialog).getByLabelText('新建波特率')).toHaveValue(9600);
    expect(within(dialog).getByLabelText('新建数据位')).toHaveValue('8');
    expect(within(dialog).getByLabelText('新建停止位')).toHaveValue('1');
    expect(within(dialog).getByLabelText('新建校验位')).toHaveValue('even');
    fireEvent.change(within(dialog).getByLabelText('新建IEC 101 CP56 时区偏移'), {
      target: { value: '480' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    await waitFor(() => {
      expect(onCreateConnection).toHaveBeenCalledWith('edge-dev', {
        endpoint: '/dev/ttyUSB0',
        protocolType: 'Iec101',
        serial: {
          baudRate: 9600,
          dataBits: 8,
          parity: 'even',
          port: '/dev/ttyUSB0',
          stopBits: 1,
        },
        iec101: { cp56TimeZoneOffsetMinutes: 480 },
        opcUa: null,
        circuitBreaker: circuitBreakerFixture,
      });
    });
  });

  it('edits IEC 101 station timezone with the serial connection', async () => {
    const onSaveConnection = vi.fn().mockResolvedValue(undefined);
    render(
      <ProtocolConnectionsPage
        connections={[
          {
            ...connectionFixture,
            connectionId: 'iec101-station-a',
            protocol: 'IEC-101',
            protocolType: 'Iec101',
            endpoint: '/dev/ttyUSB1',
            serial: {
              baudRate: 9600,
              dataBits: 8,
              parity: 'even',
              port: '/dev/ttyUSB1',
              stopBits: 1,
            },
            iec101: { cp56TimeZoneOffsetMinutes: 480 },
          },
        ]}
        selectedEdgeId="edge-dev"
        onSaveConnection={onSaveConnection}
      />,
    );

    fireEvent.click(
      screen.getByRole('button', { name: '编辑连接 iec101-station-a' }),
    );
    const dialog = screen.getByRole('dialog', { name: '编辑连接 iec101-station-a' });
    const timezone = within(dialog).getByLabelText('IEC 101 CP56 时区偏移');
    expect(timezone).toHaveValue(480);
    fireEvent.change(timezone, { target: { value: '540' } });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    await waitFor(() => {
      expect(onSaveConnection).toHaveBeenCalledWith(
        'edge-dev',
        'iec101-station-a',
        expect.objectContaining({
          endpoint: '/dev/ttyUSB1',
          protocolType: 'Iec101',
          iec101: { cp56TimeZoneOffsetMinutes: 540 },
        }),
      );
    });
  });

  it('creates an OPC UA connection with security, authentication, and session settings', async () => {
    const onCreateConnection = vi.fn().mockResolvedValue({
      ...connectionFixture,
      connectionId: 'connection-draft-2',
      protocol: 'OPC UA',
      protocolType: 'OpcUa',
      endpoint: 'opc.tcp://10.12.0.80:4840',
    });

    render(
      <ProtocolConnectionsPage
        selectedEdgeId="edge-dev"
        onCreateConnection={onCreateConnection}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '新建连接' }));
    fireEvent.change(screen.getByLabelText('新建协议类型'), {
      target: { value: 'OpcUa' },
    });
    fireEvent.change(screen.getByLabelText('新建端点'), {
      target: { value: 'opc.tcp://10.12.0.80:4840' },
    });
    fireEvent.change(screen.getByLabelText('新建OPC UA 安全策略'), {
      target: { value: 'basic256_sha256' },
    });
    expect(screen.getByLabelText('新建OPC UA 消息安全模式')).toHaveValue(
      'sign_and_encrypt',
    );
    fireEvent.change(screen.getByLabelText('新建OPC UA 身份认证'), {
      target: { value: 'username' },
    });
    fireEvent.change(screen.getByLabelText('新建OPC UA 用户名'), {
      target: { value: 'operator' },
    });
    fireEvent.change(screen.getByLabelText('新建OPC UA 密码环境变量'), {
      target: { value: 'VELAEDGE_OPCUA_PASSWORD' },
    });
    fireEvent.click(
      within(screen.getByRole('dialog', { name: '新建协议连接' })).getByRole(
        'button',
        { name: '保存' },
      ),
    );

    await waitFor(() => {
      expect(onCreateConnection).toHaveBeenCalledWith(
        'edge-dev',
        expect.objectContaining({
          endpoint: 'opc.tcp://10.12.0.80:4840',
          protocolType: 'OpcUa',
          serial: null,
          opcUa: expect.objectContaining({
            securityPolicy: 'basic256_sha256',
            messageSecurityMode: 'sign_and_encrypt',
            authMode: 'username',
            username: 'operator',
            passwordEnv: 'VELAEDGE_OPCUA_PASSWORD',
          }),
        }),
      );
    });
  });

  it('creates a BACnet/IP connection with structured transport settings', async () => {
    const onCreateConnection = vi.fn().mockResolvedValue({
      ...connectionFixture,
      connectionId: 'connection-draft-2',
      protocol: 'BACnet/IP',
      protocolType: 'BacnetIp',
      endpoint: '10.12.0.40:47808',
    });

    render(
      <ProtocolConnectionsPage
        selectedEdgeId="edge-dev"
        onCreateConnection={onCreateConnection}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '新建连接' }));
    fireEvent.change(screen.getByLabelText('新建协议类型'), {
      target: { value: 'BacnetIp' },
    });
    expect(screen.getByLabelText('新建端点')).toHaveValue('127.0.0.1:47808');
    fireEvent.change(screen.getByLabelText('新建端点'), {
      target: { value: '10.12.0.40:47808' },
    });
    fireEvent.change(screen.getByLabelText('新建BACnet 广播地址'), {
      target: { value: '10.12.0.255' },
    });
    fireEvent.change(screen.getByLabelText('新建BACnet APDU 超时'), {
      target: { value: '2400' },
    });
    fireEvent.change(screen.getByLabelText('新建BACnet APDU 重试'), {
      target: { value: '4' },
    });
    fireEvent.click(screen.getByLabelText('新建BACnet BBMD 外部设备'));
    fireEvent.change(screen.getByLabelText('新建BACnet BBMD 地址'), {
      target: { value: '10.12.0.10:47808' },
    });
    fireEvent.change(screen.getByLabelText('新建BACnet BBMD TTL'), {
      target: { value: '180' },
    });
    fireEvent.click(screen.getByLabelText('新建BACnet COV 变化订阅'));
    fireEvent.change(screen.getByLabelText('新建BACnet COV 租期'), {
      target: { value: '600' },
    });
    fireEvent.change(screen.getByLabelText('新建BACnet COV 降级轮询'), {
      target: { value: '45000' },
    });
    fireEvent.click(screen.getByLabelText('新建BACnet COV 确认型通知'));
    fireEvent.click(
      within(screen.getByRole('dialog', { name: '新建协议连接' })).getByRole(
        'button',
        { name: '保存' },
      ),
    );

    await waitFor(() => {
      expect(onCreateConnection).toHaveBeenCalledWith(
        'edge-dev',
        expect.objectContaining({
          endpoint: '10.12.0.40:47808',
          protocolType: 'BacnetIp',
          serial: null,
          opcUa: null,
          bacnetIp: expect.objectContaining({
            bindAddress: '0.0.0.0',
            broadcastAddress: '10.12.0.255',
            apduTimeoutMs: 2400,
            apduRetries: 4,
            maxApduLength: 1476,
            foreignDevice: {
              bbmdAddress: '10.12.0.10:47808',
              ttlSeconds: 180,
            },
            cov: {
              lifetimeSeconds: 600,
              confirmedNotifications: true,
              fallbackPollIntervalMs: 45_000,
            },
          }),
        }),
      );
    });
  });

  it('creates a Siemens S7 connection with rack, slot, PDU, and timeout settings', async () => {
    const onCreateConnection = vi.fn().mockResolvedValue({
      ...connectionFixture,
      connectionId: 's7-line-a',
      protocol: 'Siemens S7',
      protocolType: 'SiemensS7',
      endpoint: '10.12.0.30:102',
    });

    render(
      <ProtocolConnectionsPage
        selectedEdgeId="edge-dev"
        onCreateConnection={onCreateConnection}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '新建连接' }));
    const dialog = screen.getByRole('dialog', { name: '新建协议连接' });
    fireEvent.change(within(dialog).getByLabelText('新建协议类型'), {
      target: { value: 'SiemensS7' },
    });
    expect(within(dialog).getByLabelText('新建端点')).toHaveValue('127.0.0.1:102');
    fireEvent.change(within(dialog).getByLabelText('新建端点'), {
      target: { value: '10.12.0.30:102' },
    });
    fireEvent.change(within(dialog).getByLabelText('新建S7 Rack'), {
      target: { value: '1' },
    });
    fireEvent.change(within(dialog).getByLabelText('新建S7 Slot'), {
      target: { value: '2' },
    });
    fireEvent.change(within(dialog).getByLabelText('新建S7 PDU 大小'), {
      target: { value: '960' },
    });
    fireEvent.change(within(dialog).getByLabelText('新建S7 请求超时'), {
      target: { value: '12000' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    await waitFor(() => {
      expect(onCreateConnection).toHaveBeenCalledWith(
        'edge-dev',
        expect.objectContaining({
          endpoint: '10.12.0.30:102',
          protocolType: 'SiemensS7',
          serial: null,
          opcUa: null,
          siemensS7: {
            rack: 1,
            slot: 2,
            pduSize: 960,
            connectTimeoutMs: 5_000,
            requestTimeoutMs: 12_000,
          },
        }),
      );
    });
  });

  it('loads and saves existing Siemens S7 settings in the editor', async () => {
    const onSaveConnection = vi.fn().mockResolvedValue(undefined);
    render(
      <ProtocolConnectionsPage
        connections={[
          {
            ...connectionFixture,
            connectionId: 's7-line-a',
            protocol: 'Siemens S7',
            protocolType: 'SiemensS7',
            endpoint: '10.12.0.30:102',
            siemensS7: {
              rack: 0,
              slot: 2,
              pduSize: 480,
              connectTimeoutMs: 6_000,
              requestTimeoutMs: 9_000,
            },
          },
        ]}
        selectedEdgeId="edge-dev"
        onSaveConnection={onSaveConnection}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '修改连接 s7-line-a' }));
    expect(screen.getByLabelText('S7 Slot')).toHaveValue(2);
    expect(screen.getByLabelText('S7 PDU 大小')).toHaveValue('480');
    fireEvent.change(screen.getByLabelText('S7 Slot'), { target: { value: '3' } });
    fireEvent.click(
      within(screen.getByRole('dialog', { name: '编辑连接 s7-line-a' })).getByRole(
        'button',
        { name: '保存' },
      ),
    );

    await waitFor(() => {
      expect(onSaveConnection).toHaveBeenCalledWith(
        'edge-dev',
        's7-line-a',
        expect.objectContaining({
          endpoint: '10.12.0.30:102',
          protocolType: 'SiemensS7',
          siemensS7: expect.objectContaining({ slot: 3, pduSize: 480 }),
        }),
      );
    });
  });

  it('creates an Omron FINS/TCP connection with handshake routing and word order', async () => {
    const onCreateConnection = vi.fn().mockResolvedValue({
      ...connectionFixture,
      connectionId: 'fins-line-a',
      protocol: 'Omron FINS',
      protocolType: 'OmronFins',
      endpoint: '10.12.0.31:9600',
    });

    render(
      <ProtocolConnectionsPage
        selectedEdgeId="edge-dev"
        onCreateConnection={onCreateConnection}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '新建连接' }));
    const dialog = screen.getByRole('dialog', { name: '新建协议连接' });
    fireEvent.change(within(dialog).getByLabelText('新建协议类型'), {
      target: { value: 'OmronFins' },
    });
    expect(within(dialog).getByLabelText('新建端点')).toHaveValue('127.0.0.1:9600');
    fireEvent.change(within(dialog).getByLabelText('新建FINS 传输方式'), {
      target: { value: 'tcp' },
    });
    fireEvent.change(within(dialog).getByLabelText('新建端点'), {
      target: { value: '10.12.0.31:9600' },
    });
    fireEvent.change(within(dialog).getByLabelText('新建FINS 源网络号'), {
      target: { value: '1' },
    });
    fireEvent.change(within(dialog).getByLabelText('新建FINS 源节点号'), {
      target: { value: '0' },
    });
    fireEvent.change(within(dialog).getByLabelText('新建FINS 目标网络号'), {
      target: { value: '2' },
    });
    fireEvent.change(within(dialog).getByLabelText('新建FINS 目标节点号'), {
      target: { value: '0' },
    });
    fireEvent.change(within(dialog).getByLabelText('新建FINS 请求超时（ms）'), {
      target: { value: '4200' },
    });
    fireEvent.change(within(dialog).getByLabelText('新建FINS 双字字序'), {
      target: { value: 'high_word_first' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    await waitFor(() => {
      expect(onCreateConnection).toHaveBeenCalledWith(
        'edge-dev',
        expect.objectContaining({
          endpoint: '10.12.0.31:9600',
          protocolType: 'OmronFins',
          serial: null,
          opcUa: null,
          omronFins: {
            transport: 'tcp',
            sourceNetwork: 1,
            sourceNode: 0,
            sourceUnit: 0,
            destinationNetwork: 2,
            destinationNode: 0,
            destinationUnit: 0,
            timeoutMs: 4200,
            wordOrder: 'high_word_first',
          },
        }),
      );
    });
  });
});
