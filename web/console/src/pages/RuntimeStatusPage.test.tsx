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
        },
      ],
      local_store: {
        backend: 'jsonl',
        buffered_records: 0,
        oldest_buffer_age_seconds: 0,
        disk_usage_percent: 35,
      },
      algorithms: [],
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
    expect(screen.getByText('modbus.timeout')).toBeInTheDocument();
  });
});
