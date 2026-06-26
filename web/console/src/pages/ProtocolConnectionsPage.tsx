import { Plus, ShieldCheck } from 'lucide-react';

import type { ProtocolConnectionResponse } from '../api/types';

const fallbackConnections: ProtocolConnectionResponse[] = [
  {
    edgeId: 'edge-dev',
    connectionId: 'modbus-line-a',
    protocol: 'Modbus TCP',
    endpoint: '10.12.0.20:502',
    status: '启用',
    policy: '1000ms timeout / 3 retry',
  },
];

export function ProtocolConnectionsPage({
  connections = fallbackConnections,
}: {
  connections?: ProtocolConnectionResponse[];
}) {
  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>协议连接实例</h2>
          <p>
            云端维护可复用连接模板，保存时做字段和密钥校验，边端收到配置后使用本地适配器建立真实连接。
          </p>
        </div>
        <div className="toolbar">
          <button className="secondary-button" type="button">
            <ShieldCheck size={15} aria-hidden="true" />
            校验连接
          </button>
          <button className="primary-button" type="button">
            <Plus size={15} aria-hidden="true" />
            新建连接
          </button>
        </div>
      </section>

      <section className="panel">
        <div className="panel-header">
          <h3>连接清单</h3>
          <span>5 类协议能力</span>
        </div>
        <div className="table-wrap">
          <table className="ops-table">
            <thead>
              <tr>
                <th>Connection ID</th>
                <th>协议</th>
                <th>端点</th>
                <th>状态</th>
                <th>策略</th>
              </tr>
            </thead>
            <tbody>
              {connections.map((connection) => (
                <tr key={`${connection.edgeId}:${connection.connectionId}`}>
                  <td>{connection.connectionId}</td>
                  <td>{connection.protocol}</td>
                  <td>{connection.endpoint}</td>
                  <td>
                    <span className={connection.status === '启用' ? 'tag ok' : 'tag warn'}>
                      {connection.status}
                    </span>
                  </td>
                  <td>{connection.policy}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}
