import { useState } from 'react';
import {
  Activity,
  ChevronLeft,
  ChevronRight,
  Settings2,
} from 'lucide-react';

import type { EdgeNodeResponse } from '../api/types';

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
  onMonitorEdge,
  pageSize = 10,
}: {
  edges?: EdgeNodeResponse[];
  onConfigureEdge?: (edgeId: string) => void;
  onMonitorEdge?: (edgeId: string) => void;
  pageSize?: number;
}) {
  const [page, setPage] = useState(1);
  const totalPages = Math.max(1, Math.ceil(edges.length / pageSize));
  const currentPage = Math.min(page, totalPages);
  const pageStart = (currentPage - 1) * pageSize;
  const visibleEdges = edges.slice(pageStart, pageStart + pageSize);

  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>边端生命周期</h2>
          <p>
            边端由 runtime 通过 EdgeLink 主动连接后自动登记。云端只负责查看运行状态和进入该边端的数据配置。
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
    </div>
  );
}
