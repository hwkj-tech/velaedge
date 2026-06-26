import { Plus, ShieldCheck } from 'lucide-react';

const connections = [
  ['sim-main', 'Simulated', 'runtime://simulated', '启用', '1s timeout / 3 retry'],
  ['modbus-line-a', 'Modbus TCP', '10.12.0.20:502', '启用', '800ms timeout / 2 retry'],
  ['opcua-lab', 'OPC UA', 'opc.tcp://10.12.0.31:4840', '禁用', 'Basic256Sha256'],
];

export function ProtocolConnectionsPage() {
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
              {connections.map(([id, protocol, endpoint, status, policy]) => (
                <tr key={id}>
                  <td>{id}</td>
                  <td>{protocol}</td>
                  <td>{endpoint}</td>
                  <td>
                    <span className={status === '启用' ? 'tag ok' : 'tag warn'}>
                      {status}
                    </span>
                  </td>
                  <td>{policy}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}
