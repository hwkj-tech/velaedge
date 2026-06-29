import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { DiscoveryPage } from './DiscoveryPage';

describe('DiscoveryPage', () => {
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

    render(<DiscoveryPage onRunDiscovery={onRunDiscovery} selectedEdgeId="edge-dev" />);

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
  });
});
