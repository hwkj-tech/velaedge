import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { DiscoveryPage } from './DiscoveryPage';

describe('DiscoveryPage', () => {
  it('runs serial point discovery and shows suggestions', async () => {
    const onRunDiscovery = vi.fn().mockResolvedValue({
      jobId: 'discovery-edge-dev-1',
      protocolConnectionId: 'meter-rs485-bus-1',
      discoveredPoints: [
        {
          protocolConnectionId: 'meter-rs485-bus-1',
          address: 'holding_register:40001',
          valueType: 'float32',
          sampleValues: ['220.1', '220.3'],
          confidence: 0.72,
        },
      ],
      suggestions: [
        {
          pointId: 'meter_voltage_a',
          deviceId: 'meter-1',
          semanticId: 'electric.voltage_a',
          protocolConnectionId: 'meter-rs485-bus-1',
          address: 'holding_register:40001',
          valueType: 'float32',
          unit: 'V',
          confidence: 0.82,
          evidence: '数值范围和波动特征符合 A 相电压',
        },
      ],
    });

    render(<DiscoveryPage onRunDiscovery={onRunDiscovery} selectedEdgeId="edge-dev" />);

    fireEvent.change(screen.getByLabelText('连接 ID'), {
      target: { value: 'meter-rs485-bus-1' },
    });
    fireEvent.click(screen.getByRole('button', { name: '启动探测' }));

    await waitFor(() => {
      expect(onRunDiscovery).toHaveBeenCalledWith('edge-dev', {
        addressRange: 'holding_register:40001-40002',
        connectionId: 'meter-rs485-bus-1',
      });
    });
    expect(await screen.findByText('meter_voltage_a')).toBeInTheDocument();
    expect(screen.getByText('electric.voltage_a')).toBeInTheDocument();
    expect(screen.getByText('82%')).toBeInTheDocument();
  });
});
