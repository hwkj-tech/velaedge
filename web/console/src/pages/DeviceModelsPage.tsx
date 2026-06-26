import { Plus } from 'lucide-react';

const telemetry = [
  ['pressure', '压力', 'Float', 'MPa', '0-20', '泵出口压力'],
  ['temperature', '温度', 'Float', 'C', '-20-120', '电机温度'],
  ['running', '运行状态', 'Bool', '-', '-', '设备运行布尔量'],
];

export function DeviceModelsPage() {
  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>语义设备模型</h2>
          <p>
            先定义设备语义，再在点位配置中绑定具体 Modbus、OPC UA、MQTT 或模拟地址。
          </p>
        </div>
        <button className="primary-button" type="button">
          <Plus size={15} aria-hidden="true" />
          新建设备模型
        </button>
      </section>

      <section className="panel">
        <div className="panel-header">
          <h3>pump@v1 遥测定义</h3>
          <span>命令 2 个 / 事件 3 个</span>
        </div>
        <div className="table-wrap">
          <table className="ops-table">
            <thead>
              <tr>
                <th>Telemetry ID</th>
                <th>名称</th>
                <th>数据类型</th>
                <th>单位</th>
                <th>范围</th>
                <th>说明</th>
              </tr>
            </thead>
            <tbody>
              {telemetry.map((row) => (
                <tr key={row[0]}>
                  {row.map((cell) => (
                    <td key={cell}>{cell}</td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}
