import { useState } from 'react';
import { Radar } from 'lucide-react';

import type {
  DiscoveryReportResponse,
  PointMappingSuggestionResponse,
  RunDiscoveryRequest,
} from '../api/types';

export function DiscoveryPage({
  onRunDiscovery,
  selectedEdgeId = 'edge-dev',
  suggestions = [],
}: {
  onRunDiscovery?: (
    edgeId: string,
    request: RunDiscoveryRequest,
  ) => Promise<DiscoveryReportResponse> | DiscoveryReportResponse;
  selectedEdgeId?: string;
  suggestions?: PointMappingSuggestionResponse[];
}) {
  const [connectionId, setConnectionId] = useState('meter-rs485-bus-1');
  const [addressRange, setAddressRange] = useState('holding_register:40001-40002');
  const [rows, setRows] = useState(suggestions);
  const [status, setStatus] = useState<'idle' | 'running' | 'done' | 'error'>('idle');

  const handleRun = async () => {
    setStatus('running');
    try {
      const report = await onRunDiscovery?.(selectedEdgeId, {
        addressRange,
        connectionId,
      });
      setRows(report?.suggestions ?? []);
      setStatus('done');
    } catch {
      setStatus('error');
    }
  };

  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>串口点位探测</h2>
          <p>
            由云端下发受控只读探测任务，runtime 上报采样证据，Agent 生成点位映射候选配置。
          </p>
        </div>
        <div className="toolbar">
          <span className={`release-status ${status}`} role="status">
            {statusText(status)}
          </span>
          <button
            className="primary-button"
            disabled={status === 'running'}
            onClick={() => {
              void handleRun();
            }}
            type="button"
          >
            <Radar size={15} aria-hidden="true" />
            {status === 'running' ? '探测中' : '启动探测'}
          </button>
        </div>
      </section>

      <section className="panel">
        <div className="panel-header">
          <h3>探测任务</h3>
          <span>{selectedEdgeId} · 只读探测</span>
        </div>
        <div className="editor-grid">
          <label className="editor-control">
            <span>连接 ID</span>
            <input
              aria-label="连接 ID"
              value={connectionId}
              onChange={(event) => setConnectionId(event.target.value)}
            />
          </label>
          <label className="editor-control">
            <span>地址范围</span>
            <input
              aria-label="地址范围"
              value={addressRange}
              onChange={(event) => setAddressRange(event.target.value)}
            />
          </label>
        </div>
      </section>

      <section className="panel">
        <div className="panel-header">
          <h3>Agent 候选点位</h3>
          <span>{rows.length} 个建议</span>
        </div>
        <div className="table-wrap">
          <table className="ops-table">
            <thead>
              <tr>
                <th>Point ID</th>
                <th>语义</th>
                <th>地址</th>
                <th>类型</th>
                <th>单位</th>
                <th>置信度</th>
                <th>证据</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <tr key={`${row.protocolConnectionId}:${row.pointId}`}>
                  <td>{row.pointId}</td>
                  <td>{row.semanticId}</td>
                  <td>{row.address}</td>
                  <td>{row.valueType}</td>
                  <td>{row.unit}</td>
                  <td>{formatConfidence(row.confidence)}</td>
                  <td>{row.evidence}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}

function statusText(status: 'idle' | 'running' | 'done' | 'error') {
  if (status === 'running') return 'runtime 正在执行探测';
  if (status === 'done') return '探测结果已生成';
  if (status === 'error') return '探测失败';
  return '等待探测';
}

function formatConfidence(value: number) {
  return `${Math.round(value * 100)}%`;
}
