const capabilities = [
  ['协议适配器', 'Simulated, Modbus TCP, OPC UA, MQTT', '正常'],
  ['本地存储', 'JSONL buffer / 7 天保留', '正常'],
  ['配置影子', 'desired 与 reported 对齐', '正常'],
  ['边端策略', '禁止未审批命令执行', '正常'],
];

export function RuntimeStatusPage() {
  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>边端运行状态</h2>
          <p>
            观察 runtime 能力、协议适配器、本地存储、配置影子和云端同步状态，用于发布后的闭环确认。
          </p>
        </div>
      </section>

      <section className="panel">
        <div className="panel-header">
          <h3>edge-shanghai-01 能力上报</h3>
          <span>18 秒前</span>
        </div>
        <div className="table-wrap">
          <table className="ops-table">
            <thead>
              <tr>
                <th>模块</th>
                <th>状态</th>
                <th>健康</th>
              </tr>
            </thead>
            <tbody>
              {capabilities.map(([moduleName, status, health]) => (
                <tr key={moduleName}>
                  <td>{moduleName}</td>
                  <td>{status}</td>
                  <td>
                    <span className="tag ok">{health}</span>
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
