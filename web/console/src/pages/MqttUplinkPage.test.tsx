import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { MqttUplinkPage } from './MqttUplinkPage';

describe('MqttUplinkPage', () => {
  it('saves velaMQ uplink settings through the handler', async () => {
    const onSave = vi.fn().mockResolvedValue({
      sinkId: 'velamq-main',
      broker: 'mqtts://velamq.prod:8883',
      clientId: 'edge-dev-runtime-dev',
      username: 'edge-device',
      passwordEnv: 'EDGEOPS_MQTT_PASSWORD',
      tlsCaPath: '/etc/edgeops/velamq-ca.pem',
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

    fireEvent.change(screen.getByLabelText('传输安全'), {
      target: { value: 'mqtts' },
    });
    fireEvent.change(screen.getByLabelText('Broker 主机'), {
      target: { value: 'velamq.prod' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'MQTT 5.0' }));
    fireEvent.change(screen.getByLabelText('用户名（可选）'), {
      target: { value: 'edge-device' },
    });
    fireEvent.change(screen.getByLabelText('密码环境变量'), {
      target: { value: 'EDGEOPS_MQTT_PASSWORD' },
    });
    fireEvent.change(screen.getByLabelText('私有 CA 证书路径（可选）'), {
      target: { value: '/etc/edgeops/velamq-ca.pem' },
    });
    fireEvent.click(screen.getByRole('tab', { name: 'MQTT 5' }));
    fireEvent.change(screen.getByLabelText('Session Expiry（秒）'), {
      target: { value: '3600' },
    });
    fireEvent.change(screen.getByLabelText('Receive Maximum'), {
      target: { value: '32' },
    });
    fireEvent.change(screen.getByLabelText('Maximum Packet Size（字节）'), {
      target: { value: '1048576' },
    });
    fireEvent.change(screen.getByLabelText('Topic Alias Maximum'), {
      target: { value: '16' },
    });
    fireEvent.click(screen.getByLabelText('Request Response Information'));
    fireEvent.click(screen.getByRole('button', { name: '添加属性' }));
    fireEvent.change(screen.getByLabelText('属性 1 Key'), {
      target: { value: 'tenant' },
    });
    fireEvent.change(screen.getByLabelText('属性 1 Value'), {
      target: { value: 'factory-a' },
    });

    fireEvent.click(screen.getByRole('tab', { name: '遗嘱消息' }));
    fireEvent.click(screen.getByLabelText('启用遗嘱消息'));
    fireEvent.change(screen.getByLabelText('Will Topic'), {
      target: { value: 'edge/edge-dev/status' },
    });
    fireEvent.change(screen.getByLabelText('Will Payload'), {
      target: { value: '{"status":"offline"}' },
    });
    fireEvent.change(screen.getByLabelText('Will QoS'), {
      target: { value: '1' },
    });
    fireEvent.click(screen.getByLabelText('Will Retain'));
    fireEvent.change(screen.getByLabelText('Will Delay（秒）'), {
      target: { value: '10' },
    });
    fireEvent.change(screen.getByLabelText('Message Expiry（秒）'), {
      target: { value: '300' },
    });
    fireEvent.change(screen.getByLabelText('Content Type（可选）'), {
      target: { value: 'application/json' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存连接' }));

    await waitFor(() => {
      expect(onSave).toHaveBeenCalledWith(
        'edge-dev',
        expect.objectContaining({
          broker: 'mqtts://velamq.prod:8883',
          lastWill: expect.objectContaining({
            contentType: 'application/json',
            delayIntervalSeconds: 10,
            messageExpiryIntervalSeconds: 300,
            payload: '{"status":"offline"}',
            qos: 1,
            retain: true,
            topic: 'edge/edge-dev/status',
          }),
          maximumPacketSizeBytes: 1048576,
          passwordEnv: 'EDGEOPS_MQTT_PASSWORD',
          protocolVersion: '5.0',
          receiveMaximum: 32,
          requestResponseInformation: true,
          sessionExpiryIntervalSeconds: 3600,
          tlsCaPath: '/etc/edgeops/velamq-ca.pem',
          topicAliasMaximum: 16,
          username: 'edge-device',
          userProperties: [{ key: 'tenant', value: 'factory-a' }],
        }),
      );
    });
    expect(screen.queryByLabelText('Topic 模板')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('刷新间隔(ms)')).not.toBeInTheDocument();
    expect(await screen.findByText('MQTT 连接已保存')).toBeInTheDocument();
  });
});
