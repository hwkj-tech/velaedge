import { useEffect, useState } from 'react';
import { Save } from 'lucide-react';

import type { MqttUplinkResponse } from '../api/types';
import { displayError } from '../utils/errors';

const emptyUplink: MqttUplinkResponse = {
  sinkId: '',
  broker: '',
  clientId: '',
  topicTemplate: 'edge/{edge_id}/device/{device_id}/telemetry',
  qos: 1,
  batchSize: 100,
  flushIntervalMs: 1000,
};

export function MqttUplinkPage({
  onSave,
  selectedEdgeId = '',
  uplink = emptyUplink,
}: {
  onSave?: (
    edgeId: string,
    request: MqttUplinkResponse,
  ) => Promise<MqttUplinkResponse> | MqttUplinkResponse;
  selectedEdgeId?: string;
  uplink?: MqttUplinkResponse;
}) {
  const [form, setForm] = useState(uplink);
  const [saveState, setSaveState] = useState<'idle' | 'saving' | 'saved' | 'error'>(
    'idle',
  );
  const [saveMessage, setSaveMessage] = useState('');

  useEffect(() => {
    setForm(uplink);
  }, [uplink]);

  const handleSave = async () => {
    setSaveState('saving');
    setSaveMessage('');
    try {
      const saved = await onSave?.(selectedEdgeId, form);
      if (saved) {
        setForm(saved);
      }
      setSaveState('saved');
    } catch (error) {
      setSaveState('error');
      setSaveMessage(displayError(error));
    }
  };

  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>MQTT 上报到 velaMQ</h2>
          <p>串口采集数据经 outbox 发布到 velaMQ。</p>
        </div>
        <div className="toolbar">
          <span className={`release-status ${saveState}`} role="status">
            {saveState === 'error' && saveMessage
              ? `保存失败：${saveMessage}`
              : saveStateText(saveState)}
          </span>
          <button
            className="primary-button"
            disabled={saveState === 'saving' || !selectedEdgeId}
            onClick={() => {
              void handleSave();
            }}
            type="button"
          >
            <Save size={15} aria-hidden="true" />
            {saveState === 'saving' ? '保存中' : '保存上报配置'}
          </button>
        </div>
      </section>

      <section className="panel">
        <div className="panel-header">
          <h3>上报配置</h3>
          <span>{selectedEdgeId} · QoS {form.qos}</span>
        </div>
        <div className="editor-grid">
          <label className="editor-control">
            <span>Sink ID</span>
            <input
              aria-label="Sink ID"
              value={form.sinkId}
              onChange={(event) => setForm({ ...form, sinkId: event.target.value })}
            />
          </label>
          <label className="editor-control">
            <span>Broker 地址</span>
            <input
              aria-label="Broker 地址"
              value={form.broker}
              onChange={(event) => setForm({ ...form, broker: event.target.value })}
            />
          </label>
          <label className="editor-control">
            <span>Client ID</span>
            <input
              aria-label="Client ID"
              value={form.clientId}
              onChange={(event) => setForm({ ...form, clientId: event.target.value })}
            />
          </label>
          <label className="editor-control">
            <span>用户名（可选）</span>
            <input
              aria-label="MQTT 用户名"
              autoComplete="off"
              value={form.username ?? ''}
              onChange={(event) => setForm({ ...form, username: event.target.value })}
            />
          </label>
          <label className="editor-control">
            <span>密码环境变量</span>
            <input
              aria-label="密码环境变量"
              autoComplete="off"
              placeholder="EDGEOPS_MQTT_PASSWORD"
              value={form.passwordEnv ?? ''}
              onChange={(event) =>
                setForm({ ...form, passwordEnv: event.target.value })
              }
            />
          </label>
          <label className="editor-control">
            <span>私有 CA 路径</span>
            <input
              aria-label="私有 CA 路径"
              placeholder="/etc/edgeops/velamq-ca.pem"
              value={form.tlsCaPath ?? ''}
              onChange={(event) =>
                setForm({ ...form, tlsCaPath: event.target.value })
              }
            />
          </label>
          <label className="editor-control">
            <span>Topic 模板</span>
            <input
              aria-label="Topic 模板"
              value={form.topicTemplate}
              onChange={(event) =>
                setForm({ ...form, topicTemplate: event.target.value })
              }
            />
          </label>
          <label className="editor-control">
            <span>QoS</span>
            <select
              aria-label="QoS"
              value={form.qos}
              onChange={(event) => setForm({ ...form, qos: Number(event.target.value) })}
            >
              <option value={0}>0</option>
              <option value={1}>1</option>
              <option value={2}>2</option>
            </select>
          </label>
          <label className="editor-control">
            <span>批量条数</span>
            <input
              aria-label="批量条数"
              type="number"
              value={form.batchSize}
              onChange={(event) =>
                setForm({ ...form, batchSize: Number(event.target.value) })
              }
            />
          </label>
          <label className="editor-control">
            <span>刷新间隔(ms)</span>
            <input
              aria-label="刷新间隔(ms)"
              type="number"
              value={form.flushIntervalMs}
              onChange={(event) =>
                setForm({ ...form, flushIntervalMs: Number(event.target.value) })
              }
            />
          </label>
        </div>
      </section>
    </div>
  );
}

function saveStateText(state: 'idle' | 'saving' | 'saved' | 'error') {
  if (state === 'saving') return '正在保存';
  if (state === 'saved') return 'MQTT 上报配置已保存';
  if (state === 'error') return '保存失败';
  return '等待保存';
}
