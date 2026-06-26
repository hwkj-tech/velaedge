import { Activity, Cloud, Server } from 'lucide-react';
import { useEffect, useState } from 'react';

import { fetchSummary } from './api/client';
import type { SummaryResponse } from './api/types';

const initialSummary: SummaryResponse = {
  edge_count: 0,
  pending_release_count: 0,
};

export default function App() {
  const [summary, setSummary] = useState(initialSummary);
  const [loadState, setLoadState] = useState<'loading' | 'ready' | 'error'>(
    'loading',
  );

  useEffect(() => {
    let mounted = true;

    fetchSummary()
      .then((nextSummary) => {
        if (mounted) {
          setSummary(nextSummary);
          setLoadState('ready');
        }
      })
      .catch(() => {
        if (mounted) {
          setLoadState('error');
        }
      });

    return () => {
      mounted = false;
    };
  }, []);

  return (
    <main className="console-preview">
      <section className="preview-panel" aria-labelledby="preview-title">
        <div className="preview-panel__eyebrow">
          <Cloud size={16} aria-hidden="true" />
          云端边缘管理台
        </div>
        <h1 id="preview-title">EdgeOps Console</h1>
        <p>
          集中配置边端设备协议、点位映射、采集任务与配置发布，边端 runtime
          负责执行采集、算法和本地存储。
        </p>

        <div className="summary-grid" aria-label="运行摘要">
          <article>
            <Server size={18} aria-hidden="true" />
            <span>边端实例</span>
            <strong>{summary.edge_count}</strong>
          </article>
          <article>
            <Activity size={18} aria-hidden="true" />
            <span>待发布配置</span>
            <strong>{summary.pending_release_count}</strong>
          </article>
        </div>

        <p className="load-state" role="status">
          {loadState === 'loading' && '正在连接云端 API'}
          {loadState === 'ready' && '云端 API 已连接'}
          {loadState === 'error' && '暂未连接云端 API，显示默认视图'}
        </p>
      </section>
    </main>
  );
}
