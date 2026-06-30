import { useState } from 'react';
import {
  Activity,
  ChevronLeft,
  ChevronRight,
  KeyRound,
  Settings2,
  Wrench,
  X,
} from 'lucide-react';

import type {
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
  onRotateCredentials,
  pageSize = 10,
}: {
  edges?: EdgeNodeResponse[];
  onConfigureEdge?: (edgeId: string) => void;
  onEnableMaintenance?: (
    edgeId: string,
  ) => Promise<EdgeNodeActionResponse> | EdgeNodeActionResponse;
  onMonitorEdge?: (edgeId: string) => void;
  onRotateCredentials?: (
    edgeId: string,
  ) => Promise<EdgeNodeActionResponse> | EdgeNodeActionResponse;
  pageSize?: number;
}) {
  const [actionState, setActionState] = useState<
    { type: 'idle' } | { type: 'rotating' | 'maintenance'; edgeId: string }
  >({ type: 'idle' });
  const [dialog, setDialog] = useState<
    | { type: 'credentials' | 'maintenance'; edge: EdgeNodeResponse; result?: string }
    | undefined
  >();
  const [page, setPage] = useState(1);
  const totalPages = Math.max(1, Math.ceil(edges.length / pageSize));
  const currentPage = Math.min(page, totalPages);
  const pageStart = (currentPage - 1) * pageSize;
  const visibleEdges = edges.slice(pageStart, pageStart + pageSize);

  const handleRotateCredentials = async (edge: EdgeNodeResponse) => {
    const edgeId = edge.edgeId;
    setActionState({ type: 'rotating', edgeId });

    try {
      const result = await onRotateCredentials?.(edgeId);
      setDialog({
        edge,
        type: 'credentials',
        result: result?.credentialVersion
          ? `凭证已轮换 ${result.credentialVersion}`
          : `${edgeId} 凭证已轮换`,
      });
    } catch {
      setDialog({ edge, type: 'credentials', result: '凭证轮换失败' });
    } finally {
      setActionState({ type: 'idle' });
    }
  };

  const handleEnableMaintenance = async (edge: EdgeNodeResponse) => {
    const edgeId = edge.edgeId;
    setActionState({ type: 'maintenance', edgeId });

    try {
      const result = await onEnableMaintenance?.(edgeId);
      setDialog({
        edge,
        type: 'maintenance',
        result: result?.status ? `维护模式已启用 ${result.status}` : '维护模式已启用',
      });
    } catch {
      setDialog({ edge, type: 'maintenance', result: '维护模式启用失败' });
    } finally {
      setActionState({ type: 'idle' });
    }
  };

  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>边端生命周期</h2>
          <p>
            边端由 runtime 通过 EdgeLink 主动连接后自动登记。配置、监控、凭证轮换和维护模式都在具体边端行内执行。
          </p>
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
              {visibleEdges.map((edge) => (
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
                      <button
                        aria-label={`轮换凭证 ${edge.edgeId}`}
                        className="secondary-button compact"
                        disabled={
                          actionState.type === 'rotating' && actionState.edgeId === edge.edgeId
                        }
                        onClick={() => {
                          setDialog({ type: 'credentials', edge });
                        }}
                        title="轮换凭证"
                        type="button"
                      >
                        <KeyRound size={14} aria-hidden="true" />
                        凭证
                      </button>
                      <button
                        aria-label={`维护模式 ${edge.edgeId}`}
                        className="secondary-button compact"
                        disabled={
                          actionState.type === 'maintenance' &&
                          actionState.edgeId === edge.edgeId
                        }
                        onClick={() => {
                          setDialog({ type: 'maintenance', edge });
                        }}
                        title="维护模式"
                        type="button"
                      >
                        <Wrench size={14} aria-hidden="true" />
                        维护
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <div className="pagination-bar" aria-label="边端分页">
          <span>
            第 {currentPage} / {totalPages} 页
          </span>
          <div className="row-actions">
            <button
              className="secondary-button compact"
              disabled={currentPage === 1}
              onClick={() => setPage((value) => Math.max(1, value - 1))}
              type="button"
            >
              <ChevronLeft size={14} aria-hidden="true" />
              上一页
            </button>
            <button
              className="secondary-button compact"
              disabled={currentPage === totalPages}
              onClick={() => setPage((value) => Math.min(totalPages, value + 1))}
              type="button"
            >
              <ChevronRight size={14} aria-hidden="true" />
              下一页
            </button>
          </div>
        </div>
      </section>
      {dialog ? (
        <EdgeLifecycleDialog
          actionState={actionState}
          dialog={dialog}
          onClose={() => setDialog(undefined)}
          onConfirm={() => {
            if (dialog.type === 'credentials') {
              void handleRotateCredentials(dialog.edge);
              return;
            }
            void handleEnableMaintenance(dialog.edge);
          }}
        />
      ) : null}
    </div>
  );
}

function EdgeLifecycleDialog({
  actionState,
  dialog,
  onClose,
  onConfirm,
}: {
  actionState: { type: 'idle' } | { type: 'rotating' | 'maintenance'; edgeId: string };
  dialog: {
    type: 'credentials' | 'maintenance';
    edge: EdgeNodeResponse;
    result?: string;
  };
  onClose: () => void;
  onConfirm: () => void;
}) {
  const isCredentials = dialog.type === 'credentials';
  const isRunning =
    (isCredentials && actionState.type === 'rotating') ||
    (!isCredentials && actionState.type === 'maintenance');

  return (
    <div className="modal-backdrop">
      <section
        aria-labelledby="edge-lifecycle-dialog-title"
        aria-modal="true"
        className="modal-panel compact-modal"
        role="dialog"
      >
        <div className="modal-header">
          <h3 id="edge-lifecycle-dialog-title">
            {isCredentials ? '轮换边端凭证' : '启用维护模式'}
          </h3>
          <button aria-label="关闭" className="icon-button" onClick={onClose} type="button">
            <X size={18} aria-hidden="true" />
          </button>
        </div>

        <div className="form-grid">
          <label>
            Edge ID
            <input readOnly value={dialog.edge.edgeId} />
          </label>
          <label>
            Runtime
            <input readOnly value={dialog.edge.runtimeId || '-'} />
          </label>
          <label>
            站点/分组
            <input readOnly value={dialog.edge.site || '-'} />
          </label>
          <label>
            当前状态
            <input readOnly value={dialog.edge.status} />
          </label>
        </div>

        <div className="info-panel">
          {isCredentials
            ? '确认后 Cloud 会生成新的边端访问凭证，runtime 下次连接或刷新凭证时使用新版本。'
            : '确认后 Cloud 将该边端标记为维护中，配置治理和运行监控继续可见。'}
        </div>

        {dialog.result ? (
          <div className="toolbar-status" role="status">
            {dialog.result}
          </div>
        ) : null}

        <div className="modal-actions">
          <button className="secondary-button" onClick={onClose} type="button">
            取消
          </button>
          <button className="primary-button" disabled={isRunning} onClick={onConfirm} type="button">
            {isRunning ? '执行中' : isCredentials ? '确认轮换' : '确认维护'}
          </button>
        </div>
      </section>
    </div>
  );
}
