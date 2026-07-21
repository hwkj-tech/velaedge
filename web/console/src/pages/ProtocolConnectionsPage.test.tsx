import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { ProtocolConnectionsPage } from './ProtocolConnectionsPage';

describe('ProtocolConnectionsPage', () => {
  it('shows protocol connection table and editor fields', () => {
    render(<ProtocolConnectionsPage selectedEdgeId="edge-dev" />);

    expect(screen.getByText('连接清单')).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: '编辑连接 modbus-line-a' }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '编辑连接 modbus-line-a' }));
    expect(screen.getByText('编辑连接 modbus-line-a')).toBeInTheDocument();
    expect(screen.getByLabelText('协议类型')).toBeInTheDocument();
  });

  it('shows an explicit row action for editing protocol connections', () => {
    render(<ProtocolConnectionsPage selectedEdgeId="edge-dev" />);

    expect(screen.getByRole('columnheader', { name: '操作' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '修改连接 modbus-line-a' }));

    expect(screen.getByRole('dialog', { name: '编辑连接 modbus-line-a' })).toBeInTheDocument();
    expect(screen.getByLabelText('端点')).toHaveValue('10.12.0.20:502');
  });

  it('saves edited protocol connection drafts from the editor drawer', async () => {
    const onSaveConnection = vi.fn().mockResolvedValue(undefined);

    render(
      <ProtocolConnectionsPage
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
          },
          {
            edgeId: 'edge-dev',
            connectionId: 'connection-draft-2',
            protocol: 'Modbus TCP',
            protocolType: 'ModbusTcp',
            endpoint: 'runtime://pending',
            status: '启用',
            policy: '1000ms timeout / 3 retry',
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
      });
    });
    expect(screen.getByText('已创建连接 connection-draft-2')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '编辑连接 connection-draft-2' }));
    expect(screen.getByText('编辑连接 connection-draft-2')).toBeInTheDocument();
  });

  it('uses IEC 101 serial defaults and exposes all transport settings', () => {
    render(<ProtocolConnectionsPage selectedEdgeId="edge-dev" />);

    fireEvent.click(screen.getByRole('button', { name: '新建连接' }));
    fireEvent.change(screen.getByLabelText('新建协议类型'), {
      target: { value: 'Iec101' },
    });

    expect(screen.getByLabelText('新建波特率')).toHaveValue(9600);
    expect(screen.getByLabelText('新建数据位')).toHaveValue('8');
    expect(screen.getByLabelText('新建停止位')).toHaveValue('1');
    expect(screen.getByLabelText('新建校验位')).toHaveValue('even');
  });
});
