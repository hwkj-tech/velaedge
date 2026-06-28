import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type {
  AuditRecordResponse,
  EdgeNodeResponse,
  RuntimeStatusResponse,
} from '../api/types';
import { DashboardPage } from './DashboardPage';

const edgeNodes: EdgeNodeResponse[] = [
  {
    edgeId: 'edge-dev',
    displayName: '研发实验室边端',
    site: '研发/实验室',
    runtimeId: 'runtime-dev',
    status: '健康',
    resources: '18.5% / 42% / 61%',
    heartbeat: '8 秒前',
    capabilities: ['protocol:modbus-tcp'],
  },
];

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
        active_task_count: 2,
        success_rate: 0.995,
        average_latency_ms: 24,
        bad_point_count: 1,
      },
      protocols: [],
      local_store: {
        backend: 'rocksdb',
        buffered_records: 7,
        oldest_buffer_age_seconds: 3,
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
      category: 'Collection',
      code: 'bad-point',
      message: 'pressure quality is bad',
      timestamp: '2026-06-26T10:00:01Z',
      context: {},
    },
  ],
};

const auditRecords: AuditRecordResponse[] = [
  {
    createdAt: '2026-06-26T10:00:00Z',
    time: '10:00:00',
    actor: 'system',
    action: 'publish_release',
    target: 'edge-dev',
    result: '成功',
  },
];

describe('DashboardPage', () => {
  it('renders monitoring only without management actions', () => {
    render(
      <DashboardPage
        auditRecords={auditRecords}
        edgeNodes={edgeNodes}
        loadState="ready"
        runtimeStatus={runtimeStatus}
        summary={{ edge_count: 1, pending_release_count: 2 }}
      />,
    );

    expect(screen.getByText('Dashboard')).toBeInTheDocument();
    expect(screen.getByText('在线率')).toBeInTheDocument();
    expect(screen.getByText('100%')).toBeInTheDocument();
    expect(screen.getByText('平均延迟')).toBeInTheDocument();
    expect(screen.getByText('24ms')).toBeInTheDocument();
    expect(screen.getByText('运行任务')).toBeInTheDocument();
    expect(screen.getByText('2')).toBeInTheDocument();
    expect(screen.getByText('pressure quality is bad')).toBeInTheDocument();
    expect(screen.getByText('publish_release')).toBeInTheDocument();

    expect(screen.queryByRole('button', { name: '创建点位' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '发布配置' })).not.toBeInTheDocument();
    expect(screen.queryByText('快捷操作')).not.toBeInTheDocument();
  });
});
