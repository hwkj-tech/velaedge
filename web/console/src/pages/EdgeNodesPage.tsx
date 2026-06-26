import { KeyRound, Plus, Wrench } from 'lucide-react';

const rows = [
  ['edge-shanghai-01', '上海一厂边端', '华东/产线 A', '0.1.0', '在线', '52% / 61% / 44%', '18 秒前'],
  ['edge-suzhou-02', '苏州测试线', '华东/测试线', '0.1.0', '在线', '37% / 48% / 39%', '24 秒前'],
  ['edge-lab-03', '研发实验室', '研发/实验室', '0.1.0', '维护', '12% / 28% / 22%', '11 分钟前'],
];

export function EdgeNodesPage() {
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
          <span>3 个实例</span>
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
              {rows.map(([edgeId, name, group, runtime, status, resources, heartbeat]) => (
                <tr key={edgeId}>
                  <td>{edgeId}</td>
                  <td>{name}</td>
                  <td>{group}</td>
                  <td>{runtime}</td>
                  <td>
                    <span className={status === '在线' ? 'tag ok' : 'tag warn'}>
                      {status}
                    </span>
                  </td>
                  <td>{resources}</td>
                  <td>{heartbeat}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}
