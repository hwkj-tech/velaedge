import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { DiscoveryPage } from './DiscoveryPage';

describe('DiscoveryPage', () => {
  it('only offers connections whose Runtime capability supports automatic discovery', () => {
    render(
      <DiscoveryPage
        connections={[
          {
            connectionId: 'modbus-line-a',
            edgeId: 'edge-dev',
            endpoint: '/dev/ttyUSB0',
            policy: '1000ms timeout / 3 retry',
            protocol: 'Modbus RTU',
            protocolType: 'ModbusRtu',
            status: '启用',
          },
          {
            connectionId: 's7-line-a',
            edgeId: 'edge-dev',
            endpoint: 's7://127.0.0.1:102',
            policy: 'Rack 0 / Slot 1',
            protocol: 'Siemens S7',
            protocolType: 'SiemensS7',
            status: '启用',
          },
        ]}
        protocolCatalog={[
          {
            automaticDiscovery: false,
            capabilityId: 'modbus-rtu',
            commandWrite: true,
            displayName: 'Modbus RTU',
            maturity: 'deployment_candidate',
            protocolType: 'ModbusRtu',
            telemetryRead: true,
            transport: 'serial',
          },
          {
            automaticDiscovery: true,
            capabilityId: 'siemens-s7',
            commandWrite: true,
            displayName: 'Siemens S7',
            maturity: 'deployment_candidate',
            protocolType: 'SiemensS7',
            telemetryRead: true,
            transport: 'tcp',
          },
        ]}
        selectedEdgeId="edge-dev"
      />,
    );

    const connectionSelect = screen.getByLabelText('协议连接');
    expect(within(connectionSelect).getByRole('option', { name: 's7-line-a · Siemens S7' }))
      .toBeInTheDocument();
    expect(within(connectionSelect).queryByRole('option', { name: 'modbus-line-a · Modbus RTU' }))
      .not.toBeInTheDocument();
  });

  it('runs serial point discovery and shows suggestions', async () => {
    const onRunDiscovery = vi.fn().mockResolvedValue({
      jobId: 'discovery-edge-dev-1',
      protocolConnectionId: 'modbus-line-a',
      discoveredPoints: [
        {
          protocolConnectionId: 'modbus-line-a',
          address: 'holding_register:40001',
          valueType: 'float32',
          sampleValues: ['220.1', '220.3'],
          confidence: 0.72,
        },
      ],
      suggestions: [
        {
          pointId: 'pump_flow_rate',
          deviceId: 'pump-1',
          semanticId: 'pump.flow_rate',
          protocolConnectionId: 'modbus-line-a',
          address: 'holding_register:40001',
          valueType: 'float32',
          unit: 'm3/h',
          confidence: 0.82,
          evidence: '数值范围和波动特征符合泵流量',
        },
      ],
    });

    render(
      <DiscoveryPage
        connections={[
          {
            connectionId: 'modbus-line-a',
            edgeId: 'edge-dev',
            endpoint: '/dev/ttyUSB0',
            policy: '1000ms timeout / 3 retry',
            protocol: 'Modbus RTU',
            protocolType: 'ModbusRtu',
            status: '启用',
          },
        ]}
        onRunDiscovery={onRunDiscovery}
        selectedEdgeId="edge-dev"
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '启动探测' }));

    await waitFor(() => {
      expect(onRunDiscovery).toHaveBeenCalledWith('edge-dev', {
        addressRange: 'holding_register:40001-40002',
        connectionId: 'modbus-line-a',
      });
    });
    expect(await screen.findByText('pump_flow_rate')).toBeInTheDocument();
    expect(screen.getByText('pump.flow_rate')).toBeInTheDocument();
    expect(screen.getByText('82%')).toBeInTheDocument();
    expect(screen.getAllByText('holding_register:40001')).toHaveLength(2);
  });

  it('submits OPC UA Browse parameters and renders real discovered nodes', async () => {
    const onRunDiscovery = vi.fn().mockResolvedValue({
      discoveredPoints: [
        {
          address: 'node_id:i=2258',
          confidence: 0.9,
          protocolConnectionId: 'opcua-main',
          sampleValues: ['2026-08-03T10:00:00Z'],
          valueType: 'text',
        },
      ],
      jobId: 'browse-opcua-main',
      protocolConnectionId: 'opcua-main',
      suggestions: [],
    });

    render(
      <DiscoveryPage
        connections={[
          {
            connectionId: 'opcua-main',
            edgeId: 'edge-dev',
            endpoint: 'opc.tcp://127.0.0.1:4840',
            policy: 'SignAndEncrypt',
            protocol: 'OPC UA',
            protocolType: 'OpcUa',
            status: '启用',
          },
        ]}
        onRunDiscovery={onRunDiscovery}
        selectedEdgeId="edge-dev"
      />,
    );

    fireEvent.change(screen.getByLabelText('根 NodeId'), {
      target: { value: 'ns=2;s=Factory' },
    });
    fireEvent.change(screen.getByLabelText('最大层级'), {
      target: { value: '4' },
    });
    fireEvent.click(screen.getByLabelText('包含 OPC UA 标准命名空间'));
    fireEvent.click(screen.getByRole('button', { name: '启动探测' }));

    await waitFor(() => {
      expect(onRunDiscovery).toHaveBeenCalledWith('edge-dev', {
        connectionId: 'opcua-main',
        includeStandardNamespace: true,
        maxDepth: 4,
        rootNodeId: 'ns=2;s=Factory',
      });
    });
    expect(await screen.findByText('node_id:i=2258')).toBeInTheDocument();
    expect(screen.getByText('2026-08-03T10:00:00Z')).toBeInTheDocument();
  });
});
