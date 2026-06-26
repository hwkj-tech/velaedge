import { Plus, TimerReset } from 'lucide-react';

const tasks = [
  ['pump-main-collection', 'pump-1', 'pressure, temperature', '1000ms', '启用'],
  ['pump-status-collection', 'pump-1', 'running', '5000ms', '启用'],
  ['lab-diagnostics', 'lab-rig-1', 'vibration, current', '2000ms', '暂停'],
];

export function CollectionTasksPage() {
  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>采集任务编排</h2>
          <p>
            将点位组织成边端可执行的采集任务，统一配置周期、超时、重试、死区和缓存策略。
          </p>
        </div>
        <div className="toolbar">
          <button className="secondary-button" type="button">
            <TimerReset size={15} aria-hidden="true" />
            统一调度策略
          </button>
          <button className="primary-button" type="button">
            <Plus size={15} aria-hidden="true" />
            新建任务
          </button>
        </div>
      </section>

      <section className="panel">
        <div className="panel-header">
          <h3>任务清单</h3>
          <span>边端执行计划</span>
        </div>
        <div className="table-wrap">
          <table className="ops-table">
            <thead>
              <tr>
                <th>Task ID</th>
                <th>设备</th>
                <th>点位</th>
                <th>周期</th>
                <th>状态</th>
              </tr>
            </thead>
            <tbody>
              {tasks.map(([id, device, pointList, interval, status]) => (
                <tr key={id}>
                  <td>{id}</td>
                  <td>{device}</td>
                  <td>{pointList}</td>
                  <td>{interval}</td>
                  <td>
                    <span className={status === '启用' ? 'tag ok' : 'tag warn'}>
                      {status}
                    </span>
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
