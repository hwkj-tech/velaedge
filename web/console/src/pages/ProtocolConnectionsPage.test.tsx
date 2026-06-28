import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { ProtocolConnectionsPage } from './ProtocolConnectionsPage';

describe('ProtocolConnectionsPage', () => {
  it('shows protocol connection table and editor fields', () => {
    render(<ProtocolConnectionsPage selectedEdgeId="edge-dev" />);

    expect(screen.getByText('连接清单')).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: '选择连接 modbus-line-a' }),
    ).toBeInTheDocument();
    expect(screen.getByText('编辑连接 modbus-line-a')).toBeInTheDocument();
    expect(screen.getByLabelText('协议类型')).toBeInTheDocument();
  });

  it('saves edited protocol connection drafts from the editor drawer', async () => {
    const onSaveConnection = vi.fn().mockResolvedValue(undefined);

    render(
      <ProtocolConnectionsPage
        selectedEdgeId="edge-dev"
        onSaveConnection={onSaveConnection}
      />,
    );

    fireEvent.change(screen.getByLabelText('协议类型'), {
      target: { value: 'OpcUa' },
    });
    fireEvent.change(screen.getByLabelText('端点'), {
      target: { value: 'opc.tcp://10.12.0.80:4840' },
    });
    fireEvent.click(
      within(screen.getByRole('complementary', { name: '编辑连接 modbus-line-a' })).getByRole(
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

  it('switches the active edge before editing protocol connections', async () => {
    const onSelectEdge = vi.fn().mockResolvedValue(undefined);

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
        onSelectEdge={onSelectEdge}
      />,
    );

    fireEvent.change(screen.getByLabelText('配置边端'), {
      target: { value: 'edge-prod' },
    });

    await waitFor(() => {
      expect(onSelectEdge).toHaveBeenCalledWith('edge-prod');
    });
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
      message: '草稿校验已完成',
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
    expect(screen.getByLabelText('新建端点')).toBeInTheDocument();
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
    fireEvent.change(screen.getByLabelText('新建端点'), {
      target: { value: '/dev/ttyUSB1' },
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
      });
    });
    expect(screen.getByText('已创建连接 connection-draft-2')).toBeInTheDocument();
    expect(screen.getByText('编辑连接 connection-draft-2')).toBeInTheDocument();
  });
});
