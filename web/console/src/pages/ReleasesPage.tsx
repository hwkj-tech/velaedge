import { GitCompare, Send, ShieldCheck } from 'lucide-react';

const releases = [
  ['v2026.06.26-002', '草稿', 'edge-lab-03', '等待校验', '新增 2 个点位'],
  ['v2026.06.26-001', '已发布', 'edge-shanghai-01, edge-suzhou-02', '全部应用成功', '初始配置'],
  ['v2026.06.25-004', '已归档', 'edge-lab-03', '应用成功', '实验室模拟配置'],
];

export function ReleasesPage() {
  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>配置发布</h2>
          <p>
            将云端草稿打包成边端配置版本，经过校验、审批、灰度和回执确认后再扩大发布范围。
          </p>
        </div>
        <div className="toolbar">
          <button className="secondary-button" type="button">
            <GitCompare size={15} aria-hidden="true" />
            查看差异
          </button>
          <button className="secondary-button" type="button">
            <ShieldCheck size={15} aria-hidden="true" />
            校验草稿
          </button>
          <button className="primary-button" type="button">
            <Send size={15} aria-hidden="true" />
            创建发布
          </button>
        </div>
      </section>

      <section className="panel">
        <div className="panel-header">
          <h3>版本与回执</h3>
          <span>发布前人工确认</span>
        </div>
        <div className="table-wrap">
          <table className="ops-table">
            <thead>
              <tr>
                <th>版本</th>
                <th>状态</th>
                <th>目标边端</th>
                <th>应用结果</th>
                <th>变更摘要</th>
              </tr>
            </thead>
            <tbody>
              {releases.map(([version, status, target, result, summary]) => (
                <tr key={version}>
                  <td>{version}</td>
                  <td>
                    <span
                      className={
                        status === '已发布'
                          ? 'tag ok'
                          : status === '草稿'
                            ? 'tag warn'
                            : 'tag'
                      }
                    >
                      {status}
                    </span>
                  </td>
                  <td>{target}</td>
                  <td>{result}</td>
                  <td>{summary}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}
