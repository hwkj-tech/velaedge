const auditRows = [
  ['14:28:09', 'admin', 'validate_config', 'v2026.06.26-002', '通过'],
  ['14:12:41', 'agent-draft', 'propose_mapping', 'pressure_backup', '待复核'],
  ['13:52:17', 'ops', 'publish_config', 'v2026.06.26-001', '成功'],
];

export function AuditLogPage() {
  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>审计日志</h2>
          <p>
            记录配置草稿、Agent 建议、人工审批、发布动作和边端回执，保证每次变更可追溯。
          </p>
        </div>
      </section>

      <section className="panel">
        <div className="panel-header">
          <h3>最近审计事件</h3>
          <span>不可变更记录</span>
        </div>
        <div className="table-wrap">
          <table className="ops-table">
            <thead>
              <tr>
                <th>时间</th>
                <th>主体</th>
                <th>动作</th>
                <th>对象</th>
                <th>结果</th>
              </tr>
            </thead>
            <tbody>
              {auditRows.map(([time, actor, action, target, result]) => (
                <tr key={`${time}-${action}`}>
                  <td>{time}</td>
                  <td>{actor}</td>
                  <td>{action}</td>
                  <td>{target}</td>
                  <td>
                    <span className={result === '成功' || result === '通过' ? 'tag ok' : 'tag warn'}>
                      {result}
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
