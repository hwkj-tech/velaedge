import { KeyRound, Plus, Wrench } from 'lucide-react';

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
}: {
  edges?: EdgeNodeResponse[];
}) {
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
          <button className="secondary-button" type="button">
            <KeyRound size={15} aria-hidden="true" />
            轮换凭证
          </button>
          <button className="secondary-button" type="button">
            <Wrench size={15} aria-hidden="true" />
            维护模式
          </button>
          <button className="primary-button" type="button">
            <Plus size={15} aria-hidden="true" />
            注册边端
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
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}
