import { useState } from 'react';
import { Activity, KeyRound, Plus, Settings2, Wrench } from 'lucide-react';

import type {
  CreateEdgeNodeRequest,
  EdgeNodeActionResponse,
  EdgeNodeResponse,
} from '../api/types';

const fallbackEdges: EdgeNodeResponse[] = [
  {
    edgeId: 'edge-dev',
    displayName: '研发实验室边端',
    site: '研发/实验室',
    runtimeId: 'runtime-dev',
    status: '健康',
    resources: '18.5% / 42% / 61%',
    heartbeat: '8 秒前',
    capabilities: ['protocol:modbus-tcp', 'local-store:jsonl'],
  },
];

export function EdgeNodesPage({
  edges = fallbackEdges,
  onConfigureEdge,
  onEnableMaintenance,
  onMonitorEdge,
  onRegisterEdge,
  onRotateCredentials,
}: {
  edges?: EdgeNodeResponse[];
  onConfigureEdge?: (edgeId: string) => void;
  onEnableMaintenance?: (
    edgeId: string,
  ) => Promise<EdgeNodeActionResponse> | EdgeNodeActionResponse;
  onMonitorEdge?: (edgeId: string) => void;
  onRegisterEdge?: (
    request: CreateEdgeNodeRequest,
  ) => Promise<EdgeNodeResponse> | EdgeNodeResponse;
  onRotateCredentials?: (
    edgeId: string,
  ) => Promise<EdgeNodeActionResponse> | EdgeNodeActionResponse;
}) {
  const [toolbarMessage, setToolbarMessage] = useState('');
  const [actionState, setActionState] = useState<
    'idle' | 'registering' | 'rotating' | 'maintenance'
  >('idle');
  const primaryEdgeId = edges[0]?.edgeId ?? fallbackEdges[0].edgeId;

  const handleRegisterEdge = async () => {
    setActionState('registering');
    setToolbarMessage('');

    try {
      const created = await onRegisterEdge?.({
        displayName: '新边端注册草稿',
        site: '待分配',
      });
      setToolbarMessage(
        created ? `已注册边端草稿 ${created.edgeId}` : '已注册边端草稿',
      );
    } catch {
      setToolbarMessage('注册边端失败');
    } finally {
      setActionState('idle');
    }
  };

  const handleRotateCredentials = async () => {
    setActionState('rotating');
    setToolbarMessage('');

    try {
      const result = await onRotateCredentials?.(primaryEdgeId);
      setToolbarMessage(
        result?.credentialVersion
          ? `凭证已轮换 ${result.credentialVersion}`
          : '凭证已轮换',
      );
    } catch {
      setToolbarMessage('凭证轮换失败');
    } finally {
      setActionState('idle');
    }
  };

  const handleEnableMaintenance = async () => {
    setActionState('maintenance');
    setToolbarMessage('');

    try {
      const result = await onEnableMaintenance?.(primaryEdgeId);
      setToolbarMessage(
        result?.status ? `维护模式已启用 ${result.status}` : '维护模式已启用',
      );
    } catch {
      setToolbarMessage('维护模式启用失败');
    } finally {
      setActionState('idle');
    }
  };

  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>边端生命周期</h2>
          <p>
            维护边端注册、分组、凭证轮换和运行能力，云端只下发配置与策略，具体采集由边端 runtime 执行。
          </p>
        </div>
        <div className="toolbar">
          {toolbarMessage ? (
            <span className="toolbar-status" role="status">
              {toolbarMessage}
            </span>
          ) : null}
          <button
            className="secondary-button"
            disabled={actionState === 'rotating'}
            onClick={() => {
              void handleRotateCredentials();
            }}
            type="button"
          >
            <KeyRound size={15} aria-hidden="true" />
            {actionState === 'rotating' ? '轮换中' : '轮换凭证'}
          </button>
          <button
            className="secondary-button"
            disabled={actionState === 'maintenance'}
            onClick={() => {
              void handleEnableMaintenance();
            }}
            type="button"
          >
            <Wrench size={15} aria-hidden="true" />
            {actionState === 'maintenance' ? '启用中' : '维护模式'}
          </button>
          <button
            className="primary-button"
            disabled={actionState === 'registering'}
            onClick={() => {
              void handleRegisterEdge();
            }}
            type="button"
          >
            <Plus size={15} aria-hidden="true" />
            {actionState === 'registering' ? '注册中' : '注册边端'}
          </button>
        </div>
      </section>

      <section className="panel">
        <div className="panel-header">
          <h3>边端实例</h3>
          <span>{edges.length} 个实例</span>
        </div>
        <div className="table-wrap">
          <table className="ops-table">
            <thead>
              <tr>
                <th>Edge ID</th>
                <th>名称</th>
                <th>站点/分组</th>
                <th>Runtime</th>
                <th>状态</th>
                <th>CPU / 内存 / 磁盘</th>
                <th>心跳</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {edges.map((edge) => (
                <tr key={edge.edgeId}>
                  <td>{edge.edgeId}</td>
                  <td>{edge.displayName}</td>
                  <td>{edge.site}</td>
                  <td>{edge.runtimeId}</td>
                  <td>
                    <span className={edge.status === '健康' ? 'tag ok' : 'tag warn'}>
                      {edge.status}
                    </span>
                  </td>
                  <td>{edge.resources}</td>
                  <td>{edge.heartbeat}</td>
                  <td>
                    <div className="row-actions">
                      <button
                        aria-label={`配置边端 ${edge.edgeId}`}
                        className="secondary-button compact"
                        onClick={() => onConfigureEdge?.(edge.edgeId)}
                        type="button"
                      >
                        <Settings2 size={14} aria-hidden="true" />
                        配置
                      </button>
                      <button
                        aria-label={`运行监控 ${edge.edgeId}`}
                        className="secondary-button compact"
                        onClick={() => onMonitorEdge?.(edge.edgeId)}
                        type="button"
                      >
                        <Activity size={14} aria-hidden="true" />
                        监控
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}
