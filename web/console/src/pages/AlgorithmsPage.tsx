import { Plus, ShieldCheck } from 'lucide-react';

import type { AlgorithmResponse } from '../api/types';

const fallbackAlgorithms: AlgorithmResponse[] = [
  {
    edgeId: 'edge-dev',
    algorithmId: 'pump-anomaly-v1',
    version: '1.0.0',
    kind: '异常检测',
    inputs: 'pressure, running',
    outputs: 'pump.anomaly_score',
    execution: '边端本地执行',
    validation: '已通过',
  },
];

export function AlgorithmsPage({
  algorithms = fallbackAlgorithms,
}: {
  algorithms?: AlgorithmResponse[];
}) {
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
              {algorithms.map((algorithm) => (
                <tr key={`${algorithm.edgeId}:${algorithm.algorithmId}`}>
                  <td>{algorithm.algorithmId}</td>
                  <td>{algorithm.kind}</td>
                  <td>{algorithm.inputs}</td>
                  <td>{algorithm.execution}</td>
                  <td>
                    <span
                      className={algorithm.validation === '已通过' ? 'tag ok' : 'tag warn'}
                    >
                      {algorithm.validation}
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
