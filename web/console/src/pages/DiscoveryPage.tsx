import { useEffect, useMemo, useState } from 'react';
import { Radar } from 'lucide-react';

import type {
  DiscoveryReportResponse,
  DiscoveredPointResponse,
  PointMappingSuggestionResponse,
  ProtocolConnectionResponse,
  RunDiscoveryRequest,
  RuntimeProtocolDescriptor,
} from '../api/types';
import { displayError } from '../utils/errors';
import { configuredProtocolCatalog } from '../protocolCatalog';

export function DiscoveryPage({
  onRunDiscovery,
  selectedEdgeId = '',
  connections = [],
  protocolCatalog,
  suggestions = [],
}: {
  onRunDiscovery?: (
    edgeId: string,
    request: RunDiscoveryRequest,
  ) => Promise<DiscoveryReportResponse> | DiscoveryReportResponse;
  selectedEdgeId?: string;
  connections?: ProtocolConnectionResponse[];
  protocolCatalog?: RuntimeProtocolDescriptor[];
  suggestions?: PointMappingSuggestionResponse[];
}) {
  const [connectionId, setConnectionId] = useState('');
  const [addressRange, setAddressRange] = useState('holding_register:40001-40002');
  const [rootNodeId, setRootNodeId] = useState('i=85');
  const [maxDepth, setMaxDepth] = useState(3);
  const [includeStandardNamespace, setIncludeStandardNamespace] = useState(false);
  const [discoveredPoints, setDiscoveredPoints] = useState<DiscoveredPointResponse[]>([]);
  const [rows, setRows] = useState(suggestions);
  const [status, setStatus] = useState<'idle' | 'running' | 'done' | 'error'>('idle');
  const [errorText, setErrorText] = useState('');
  const availableConnections = useMemo(
    () => {
      const discoverable = new Set(
        configuredProtocolCatalog(protocolCatalog)
          .filter((protocol) => protocol.automaticDiscovery)
          .map((protocol) => protocol.protocolType),
      );
      return connections.filter(
        (connection) =>
          connection.edgeId === selectedEdgeId && discoverable.has(connection.protocolType),
      );
    },
    [connections, protocolCatalog, selectedEdgeId],
  );
  const selectedConnection = availableConnections.find(
    (connection) => connection.connectionId === connectionId,
  );
  const isOpcUa = selectedConnection?.protocolType === 'OpcUa';

  useEffect(() => {
    if (!availableConnections.some((connection) => connection.connectionId === connectionId)) {
      setConnectionId(availableConnections[0]?.connectionId ?? '');
    }
  }, [availableConnections, connectionId]);

  const handleRun = async () => {
    setStatus('running');
    setErrorText('');
    try {
      const request: RunDiscoveryRequest = isOpcUa
        ? { connectionId, includeStandardNamespace, maxDepth, rootNodeId }
        : { addressRange, connectionId };
      const report = await onRunDiscovery?.(selectedEdgeId, request);
      setDiscoveredPoints(report?.discoveredPoints ?? []);
      setRows(report?.suggestions ?? []);
      setStatus('done');
    } catch (error) {
      setStatus('error');
      setErrorText(displayError(error));
    }
  };

  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>点位发现</h2>
          <p>Runtime 对目标连接执行有界只读发现，结果来自设备实时响应。</p>
        </div>
        <div className="toolbar">
          <span className={`release-status ${status}`} role="status">
            {status === 'error' && errorText ? `探测失败：${errorText}` : statusText(status)}
          </span>
          <button
            className="primary-button"
            disabled={status === 'running' || !selectedEdgeId || !connectionId.trim()}
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
          <span>{selectedEdgeId} · Modbus RTU / OPC UA</span>
        </div>
        <div className="editor-grid">
          <label className="editor-control">
            <span>协议连接</span>
            <select
              aria-label="协议连接"
              value={connectionId}
              onChange={(event) => setConnectionId(event.target.value)}
            >
              {availableConnections.map((connection) => (
                <option key={connection.connectionId} value={connection.connectionId}>
                  {connection.connectionId} · {connection.protocol}
                </option>
              ))}
            </select>
          </label>
          {isOpcUa ? (
            <>
              <label className="editor-control">
                <span>根 NodeId</span>
                <input
                  aria-label="根 NodeId"
                  value={rootNodeId}
                  onChange={(event) => setRootNodeId(event.target.value)}
                />
              </label>
              <label className="editor-control">
                <span>最大层级</span>
                <input
                  aria-label="最大层级"
                  max={8}
                  min={1}
                  type="number"
                  value={maxDepth}
                  onChange={(event) => setMaxDepth(Number(event.target.value))}
                />
              </label>
              <label className="editor-control checkbox-control">
                <input
                  aria-label="包含 OPC UA 标准命名空间"
                  checked={includeStandardNamespace}
                  type="checkbox"
                  onChange={(event) => setIncludeStandardNamespace(event.target.checked)}
                />
                <span>包含标准命名空间 ns=0</span>
              </label>
            </>
          ) : (
            <label className="editor-control">
              <span>保持寄存器范围</span>
              <input
                aria-label="保持寄存器范围"
                value={addressRange}
                onChange={(event) => setAddressRange(event.target.value)}
              />
            </label>
          )}
        </div>
      </section>

      <section className="panel">
        <div className="panel-header">
          <h3>设备发现点位</h3>
          <span>{discoveredPoints.length} 个可读标量</span>
        </div>
        <div className="table-wrap">
          <table className="ops-table">
            <thead>
              <tr>
                <th>连接</th>
                <th>地址 / NodeId</th>
                <th>推断类型</th>
                <th>实时样本</th>
                <th>置信度</th>
              </tr>
            </thead>
            <tbody>
              {discoveredPoints.map((point) => (
                <tr key={`${point.protocolConnectionId}:${point.address}`}>
                  <td>{point.protocolConnectionId}</td>
                  <td>{point.address}</td>
                  <td>{point.valueType}</td>
                  <td>{point.sampleValues.join(', ')}</td>
                  <td>{formatConfidence(point.confidence)}</td>
                </tr>
              ))}
            </tbody>
          </table>
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
  if (status === 'running') return '正在提交 Runtime 探测任务';
  if (status === 'done') return '探测结果已生成';
  if (status === 'error') return '探测失败';
  return '等待探测';
}

function formatConfidence(value: number) {
  return `${Math.round(value * 100)}%`;
}
