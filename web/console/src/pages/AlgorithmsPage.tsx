import { Plus, ShieldCheck } from 'lucide-react';

const algorithms = [
  ['pump-anomaly-v1', '异常检测', 'pressure, temperature', '本地执行', '已通过'],
  ['energy-rollup', '聚合计算', 'current, voltage', '本地执行', '已通过'],
  ['cloud-draft-advisor', 'Agent 草稿', '配置差异', '云端辅助', '需复核'],
];

export function AlgorithmsPage() {
  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>算法配置</h2>
          <p>
            管理通用边缘算法模板、输入点位和本地执行策略。Agent 可生成草稿，但发布前必须人工复核。
          </p>
        </div>
        <div className="toolbar">
          <button className="secondary-button" type="button">
            <ShieldCheck size={15} aria-hidden="true" />
            风险评估
          </button>
          <button className="primary-button" type="button">
            <Plus size={15} aria-hidden="true" />
            新建算法
          </button>
        </div>
      </section>

      <section className="panel">
        <div className="panel-header">
          <h3>算法模板</h3>
          <span>边端本地执行</span>
        </div>
        <div className="table-wrap">
          <table className="ops-table">
            <thead>
              <tr>
                <th>Algorithm ID</th>
                <th>类型</th>
                <th>输入</th>
                <th>执行位置</th>
                <th>校验</th>
              </tr>
            </thead>
            <tbody>
              {algorithms.map(([id, kind, inputs, location, validation]) => (
                <tr key={id}>
                  <td>{id}</td>
                  <td>{kind}</td>
                  <td>{inputs}</td>
                  <td>{location}</td>
                  <td>
                    <span className={validation === '已通过' ? 'tag ok' : 'tag warn'}>
                      {validation}
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
