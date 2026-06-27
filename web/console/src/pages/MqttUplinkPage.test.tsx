import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { MqttUplinkPage } from './MqttUplinkPage';

describe('MqttUplinkPage', () => {
  it('saves velaMQ uplink settings through the handler', async () => {
    const onSave = vi.fn().mockResolvedValue({
      sinkId: 'velamq-main',
      broker: 'mqtts://velamq.prod:8883',
      clientId: 'edge-dev-runtime-dev',
      topicTemplate: 'velamq/{edge_id}/{device_id}/telemetry',
      qos: 1,
      batchSize: 200,
      flushIntervalMs: 500,
    });

    render(
      <MqttUplinkPage
        onSave={onSave}
        selectedEdgeId="edge-dev"
        uplink={{
          sinkId: 'velamq-main',
          broker: 'mqtts://velamq.local:8883',
          clientId: 'edge-dev-runtime-dev',
          topicTemplate: 'edge/{edge_id}/device/{device_id}/telemetry',
          qos: 1,
          batchSize: 100,
          flushIntervalMs: 1000,
        }}
      />,
    );

    fireEvent.change(screen.getByLabelText('Broker 地址'), {
      target: { value: 'mqtts://velamq.prod:8883' },
    });
    fireEvent.change(screen.getByLabelText('Topic 模板'), {
      target: { value: 'velamq/{edge_id}/{device_id}/telemetry' },
    });
    fireEvent.change(screen.getByLabelText('批量条数'), {
      target: { value: '200' },
    });
    fireEvent.change(screen.getByLabelText('刷新间隔(ms)'), {
      target: { value: '500' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存上报配置' }));

    await waitFor(() => {
      expect(onSave).toHaveBeenCalledWith('edge-dev', {
        sinkId: 'velamq-main',
        broker: 'mqtts://velamq.prod:8883',
        clientId: 'edge-dev-runtime-dev',
        topicTemplate: 'velamq/{edge_id}/{device_id}/telemetry',
        qos: 1,
        batchSize: 200,
        flushIntervalMs: 500,
      });
    });
    expect(await screen.findByText('MQTT 上报配置已保存')).toBeInTheDocument();
  });
});
