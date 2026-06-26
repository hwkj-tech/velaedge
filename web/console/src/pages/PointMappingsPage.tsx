import { FileInput, Plus, ShieldCheck } from 'lucide-react';

const points = [
  ['pressure', 'pump-1', 'pressure', 'Modbus TCP', 'modbus-line-a', 'holding_register 40001', 'Float', 'MPa', '1s'],
  ['temperature', 'pump-1', 'temperature', 'Simulated', 'sim-main', 'simulated temperature', 'Float', 'C', '1s'],
  ['running', 'pump-1', 'running', 'MQTT', 'mqtt-main', 'pump/1/running', 'Bool', '-', '5s'],
];

export function PointMappingsPage() {
  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>语义点位到协议地址</h2>
          <p>
            点位在云端集中配置和校验，发布后由边端 runtime 按协议适配器执行采集、缓存和质量规则。
          </p>
        </div>
        <div className="toolbar">
          <button className="secondary-button" type="button">
            <FileInput size={15} aria-hidden="true" />
            批量导入
          </button>
          <button className="secondary-button" type="button">
            <ShieldCheck size={15} aria-hidden="true" />
            校验草稿
          </button>
          <button className="primary-button" type="button">
            <Plus size={15} aria-hidden="true" />
            新建点位
          </button>
        </div>
      </section>

      <section className="panel">
        <div className="panel-header">
          <h3>点位映射预览</h3>
          <span>3 个启用点位</span>
        </div>
        <div className="table-wrap">
          <table className="ops-table">
            <thead>
              <tr>
                <th>Point ID</th>
                <th>设备</th>
                <th>语义</th>
                <th>协议</th>
                <th>连接</th>
                <th>地址</th>
                <th>类型</th>
                <th>单位</th>
                <th>周期</th>
              </tr>
            </thead>
            <tbody>
              {points.map((row) => (
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
