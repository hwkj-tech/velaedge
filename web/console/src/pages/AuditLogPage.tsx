import type { AuditRecordResponse } from '../api/types';

const fallbackAuditRecords: AuditRecordResponse[] = [
  {
    createdAt: '2026-06-26T10:00:00Z',
    time: '10:00:00',
    actor: 'system',
    action: 'create_release',
    target: '2026.06.26-001',
    result: '成功',
  },
];

export function AuditLogPage({
  auditRecords = fallbackAuditRecords,
}: {
  auditRecords?: AuditRecordResponse[];
}) {
  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>审计日志</h2>
          <p>配置、审批、发布与回执追踪。</p>
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
              {auditRecords.map((record) => (
                <tr key={`${record.createdAt}-${record.action}-${record.target}`}>
                  <td>{record.time}</td>
                  <td>{record.actor}</td>
                  <td>{record.action}</td>
                  <td>{record.target}</td>
                  <td>
                    <span className={record.result === '成功' ? 'tag ok' : 'tag warn'}>
                      {record.result}
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
