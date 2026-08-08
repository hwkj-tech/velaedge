import { useEffect, useState } from 'react';
import { Save } from 'lucide-react';

import type { MqttUplinkResponse } from '../api/types';
import { MqttConnectionForm } from '../components/MqttConnectionForm';
import { displayError } from '../utils/errors';

const emptyUplink: MqttUplinkResponse = {
  sinkId: '',
  broker: '',
  clientId: '',
  protocolVersion: '3.1.1',
  keepAliveSeconds: 60,
  cleanSession: true,
  cleanStart: true,
  sessionExpiryIntervalSeconds: 0,
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
          <h2>MQTT 连接</h2>
          <p>维护 Runtime 到 MQTT Broker 的连接、认证、TLS 与会话参数。</p>
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
            {saveState === 'saving' ? '保存中' : '保存连接'}
          </button>
        </div>
      </section>

      <section className="panel">
        <div className="panel-header">
          <h3>连接配置</h3>
          <span>{selectedEdgeId} · MQTT {form.protocolVersion ?? '3.1.1'}</span>
        </div>
        <MqttConnectionForm form={form} onChange={setForm} />
      </section>
    </div>
  );
}

function saveStateText(state: 'idle' | 'saving' | 'saved' | 'error') {
  if (state === 'saving') return '正在保存';
  if (state === 'saved') return 'MQTT 连接已保存';
  if (state === 'error') return '保存失败';
  return '等待保存';
}
