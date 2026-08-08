import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type { RuntimeStatusResponse } from '../api/types';
import { RuntimeStatusPage } from './RuntimeStatusPage';

const runtimeStatus: RuntimeStatusResponse = {
  healthyEdgeCount: 1,
  degradedEdgeCount: 0,
  criticalEdgeCount: 0,
  averageCollectionLatencyMs: 24,
  edges: [
    {
      edge_id: 'edge-dev',
      runtime_id: 'runtime-dev',
      config_version: '2026.06.26-001',
      timestamp: '2026-06-26T10:00:00Z',
      health: 'Healthy',
      system: {
        cpu_percent: 18.5,
        memory_percent: 42,
        disk_percent: 61,
        process_uptime_seconds: 3600,
      },
      collection: {
        active_task_count: 1,
        success_rate: 0.995,
        average_latency_ms: 24,
        bad_point_count: 0,
      },
      protocols: [
        {
          connection_id: 'modbus-line-a',
          protocol: 'Modbus TCP',
          connected: true,
          latency_ms: 12,
          timeout_count: 0,
          error_count: 0,
          reconnect_count: 0,
          collection_attempt_count: 120,
          collection_success_count: 119,
          write_attempt_count: 4,
          write_success_count: 3,
          circuit_state: 'Closed',
          consecutive_failure_count: 0,
          circuit_open_count: 0,
          circuit_rejected_count: 0,
          last_quality_code: 'uncertain_out_of_range',
          good_value_count: 12,
          uncertain_value_count: 1,
          bad_value_count: 0,
          subscription_count: 2,
          notification_count: 18,
          subscription_error_count: 1,
          fallback_poll_count: 3,
        },
      ],
      local_store: {
        backend: 'jsonl',
        buffered_records: 0,
        oldest_buffer_age_seconds: 0,
        disk_usage_percent: 35,
      },
      algorithms: [
        {
          algorithm_id: 'pressure-window',
          healthy: true,
          last_run_latency_ms: 4,
          error_count: 0,
          alert_count: 1,
        },
      ],
      mqtt: {
        configured_sink_count: 1,
        connected_sink_count: 1,
        connection_generation: 1,
        publish_success_count: 42,
        publish_failure_count: 1,
        published_bytes: 2048,
        sinks: [
          {
            sink_id: 'velamq-main',
            broker: 'mqtt://127.0.0.1:1883',
            client_id: 'runtime-dev',
            connected: true,
            publish_success_count: 42,
            publish_failure_count: 1,
            published_bytes: 2048,
            average_ack_latency_ms: 8,
            last_ack_latency_ms: 6,
            last_publish_at: '2026-06-26T10:01:00Z',
            last_topic: 'factory/edge-dev/pump/telemetry',
            last_error: null,
          },
        ],
      },
      cloud_sync: {
        connected: true,
        last_sync_seconds_ago: 8,
        pending_uploads: 0,
        desired_version: '2026.06.26-001',
        reported_version: '2026.06.26-001',
      },
    },
  ],
  events: [
    {
      edge_id: 'edge-dev',
      severity: 'Warning',
      category: 'Protocol',
      code: 'modbus.timeout',
      message: 'Modbus TCP read timeout',
      timestamp: '2026-06-26T10:01:00Z',
      context: { connection_id: 'modbus-line-a' },
    },
  ],
};

describe('RuntimeStatusPage', () => {
  it('renders runtime metrics from cloud API data', () => {
    render(<RuntimeStatusPage runtimeStatus={runtimeStatus} />);

    expect(screen.getAllByText('edge-dev').length).toBeGreaterThan(0);
    expect(screen.getByText('18.5%')).toBeInTheDocument();
    expect(screen.getByText('99.5%')).toBeInTheDocument();
    expect(screen.getByText('Modbus TCP')).toBeInTheDocument();
    expect(screen.getByText('采集 119/120 · 写入 3/4')).toBeInTheDocument();
    expect(screen.getByText('超量程 · G 12 / U 1 / B 0')).toBeInTheDocument();
    expect(screen.getByText('2 订阅 · 18 通知 · 1 错误 · 3 次降级')).toBeInTheDocument();
    expect(screen.getByText('MQTT 传输')).toBeInTheDocument();
    expect(screen.getByText('factory/edge-dev/pump/telemetry')).toBeInTheDocument();
    expect(screen.getByText('pressure-window')).toBeInTheDocument();
    expect(screen.getByText('modbus.timeout')).toBeInTheDocument();
  });

  it('does not fall back to unrelated edges when a focused runtime has no metrics', () => {
    render(<RuntimeStatusPage focusedEdgeId="edge-missing" runtimeStatus={runtimeStatus} />);

    expect(screen.getByText('尚未收到 edge-missing 的运行指标')).toBeInTheDocument();
    expect(screen.queryByText('modbus-line-a')).not.toBeInTheDocument();
  });
});
