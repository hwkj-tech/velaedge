import { Plus, Trash2 } from 'lucide-react';
import { useState } from 'react';

import type {
  MqttLastWill,
  MqttUplinkResponse,
  MqttUserProperty,
} from '../api/types';

type BrokerParts = {
  host: string;
  port: string;
  transport: 'mqtt' | 'mqtts';
};

type FormSection = 'connection' | 'mqtt5' | 'will';

const emptyLastWill: MqttLastWill = {
  topic: '',
  payload: '',
  qos: 0,
  retain: false,
  delayIntervalSeconds: 0,
  payloadFormatUtf8: true,
  messageExpiryIntervalSeconds: 0,
  userProperties: [],
};

export function MqttConnectionForm({
  form,
  onChange,
}: {
  form: MqttUplinkResponse;
  onChange: (form: MqttUplinkResponse) => void;
}) {
  const protocolVersion = form.protocolVersion ?? '3.1.1';
  const [activeSection, setActiveSection] = useState<FormSection>('connection');
  const broker = parseBroker(form.broker);
  const patch = (next: Partial<MqttUplinkResponse>) => onChange({ ...form, ...next });
  const updateBroker = (next: Partial<BrokerParts>) => {
    const value = { ...broker, ...next };
    patch({ broker: `${value.transport}://${value.host}${value.port ? `:${value.port}` : ''}` });
  };
  const sections: Array<{ id: FormSection; label: string }> = [
    { id: 'connection', label: '连接' },
    ...(protocolVersion === '5.0' ? [{ id: 'mqtt5' as const, label: 'MQTT 5' }] : []),
    { id: 'will', label: '遗嘱消息' },
  ];
  const visibleSection = protocolVersion === '3.1.1' && activeSection === 'mqtt5'
    ? 'connection'
    : activeSection;

  return (
    <div className="mqtt-connection-form">
      <div aria-label="MQTT 设置分类" className="mqtt-form-tabs" role="tablist">
        {sections.map((section) => (
          <button
            aria-selected={visibleSection === section.id}
            className={visibleSection === section.id ? 'active' : ''}
            key={section.id}
            onClick={() => setActiveSection(section.id)}
            role="tab"
            type="button"
          >
            {section.label}
          </button>
        ))}
      </div>

      {visibleSection === 'connection' ? (
        <div className="mqtt-form-page" role="tabpanel">
          <FormSectionBlock
            eyebrow="PROTOCOL"
            title="协议版本"
            description="选择 Runtime 与 Broker 建立连接时使用的握手协议。"
          >
            <div aria-label="MQTT 协议版本" className="mqtt-version-control" role="group">
              {(['3.1.1', '5.0'] as const).map((version) => (
                <button
                  aria-pressed={protocolVersion === version}
                  className={protocolVersion === version ? 'active' : ''}
                  key={version}
                  onClick={() => {
                    patch({ protocolVersion: version });
                    if (version === '3.1.1') setActiveSection('connection');
                  }}
                  type="button"
                >
                  MQTT {version}
                </button>
              ))}
            </div>
          </FormSectionBlock>

          <FormSectionBlock eyebrow="CONNECTION" title="连接">
            <div className="mqtt-field-grid">
              <Field label="连接名称" value={form.sinkId} onChange={(sinkId) => patch({ sinkId })} />
              <Field label="Client ID" value={form.clientId} onChange={(clientId) => patch({ clientId })} />
              <label className="editor-control">
                <span>传输安全</span>
                <select
                  aria-label="传输安全"
                  value={broker.transport}
                  onChange={(event) => {
                    const transport = event.target.value as BrokerParts['transport'];
                    const defaultPort = transport === 'mqtts' ? '8883' : '1883';
                    const previousDefaultPort = broker.transport === 'mqtts' ? '8883' : '1883';
                    updateBroker({
                      transport,
                      port: !broker.port || broker.port === previousDefaultPort ? defaultPort : broker.port,
                    });
                  }}
                >
                  <option value="mqtt">MQTT / TCP</option>
                  <option value="mqtts">MQTTS / TLS</option>
                </select>
              </label>
              <Field label="Broker 主机" placeholder="broker.example.com" value={broker.host} onChange={(host) => updateBroker({ host })} />
              <Field label="端口" min="1" type="number" value={broker.port} onChange={(port) => updateBroker({ port })} />
              <Field
                label="Keep Alive（秒）"
                min="5"
                type="number"
                value={String(form.keepAliveSeconds ?? 60)}
                onChange={(keepAliveSeconds) => patch({ keepAliveSeconds: Number(keepAliveSeconds) })}
              />
            </div>
          </FormSectionBlock>

          <FormSectionBlock
            eyebrow="AUTHENTICATION"
            title="认证与 TLS"
            description="密码由 Runtime 环境变量读取，不在云端保存明文。"
          >
            <div className="mqtt-field-grid">
              <Field label="用户名（可选）" value={form.username ?? ''} onChange={(username) => patch({ username })} />
              <Field
                label="密码环境变量"
                placeholder="VELAEDGE_MQTT_PASSWORD"
                value={form.passwordEnv ?? ''}
                onChange={(passwordEnv) => patch({ passwordEnv })}
              />
              {broker.transport === 'mqtts' ? (
                <Field
                  label="私有 CA 证书路径（可选）"
                  placeholder="/etc/velaedge/certs/ca.pem"
                  value={form.tlsCaPath ?? ''}
                  onChange={(tlsCaPath) => patch({ tlsCaPath })}
                />
              ) : null}
            </div>
          </FormSectionBlock>

          {protocolVersion === '3.1.1' ? (
            <FormSectionBlock eyebrow="SESSION" title="会话">
              <ToggleField
                checked={form.cleanSession ?? true}
                description="断开后不保留该 Client ID 的订阅和离线消息。"
                label="Clean Session"
                onChange={(cleanSession) => patch({ cleanSession })}
              />
            </FormSectionBlock>
          ) : null}
        </div>
      ) : null}

      {visibleSection === 'mqtt5' ? (
        <div className="mqtt-form-page" role="tabpanel">
          <FormSectionBlock
            eyebrow="SESSION"
            title="会话生命周期"
            description="控制 Broker 是否恢复历史会话以及断开后的保留时长。"
          >
            <div className="mqtt-session-fields">
              <ToggleField
                checked={form.cleanStart ?? true}
                description="本次连接以全新会话开始。"
                label="Clean Start"
                onChange={(cleanStart) => patch({ cleanStart })}
              />
              <OptionalNumberField
                label="Session Expiry（秒）"
                min="0"
                value={form.sessionExpiryIntervalSeconds}
                onChange={(sessionExpiryIntervalSeconds) => patch({ sessionExpiryIntervalSeconds })}
              />
            </div>
          </FormSectionBlock>

          <FormSectionBlock
            eyebrow="CONNECT PROPERTIES"
            title="连接属性"
            description="留空表示使用 Broker 或客户端默认值。"
          >
            <div className="mqtt-field-grid mqtt-field-grid-three">
              <OptionalNumberField
                label="Receive Maximum"
                min="1"
                value={form.receiveMaximum}
                onChange={(receiveMaximum) => patch({ receiveMaximum })}
              />
              <OptionalNumberField
                label="Maximum Packet Size（字节）"
                min="1"
                value={form.maximumPacketSizeBytes}
                onChange={(maximumPacketSizeBytes) => patch({ maximumPacketSizeBytes })}
              />
              <OptionalNumberField
                label="Topic Alias Maximum"
                min="0"
                value={form.topicAliasMaximum}
                onChange={(topicAliasMaximum) => patch({ topicAliasMaximum })}
              />
            </div>
            <div className="mqtt-toggle-grid">
              <ToggleField
                checked={form.requestResponseInformation ?? false}
                description="请求 Broker 返回响应信息。"
                label="Request Response Information"
                onChange={(requestResponseInformation) => patch({ requestResponseInformation })}
              />
              <ToggleField
                checked={form.requestProblemInformation ?? true}
                description="允许 Broker 返回诊断原因和用户属性。"
                label="Request Problem Information"
                onChange={(requestProblemInformation) => patch({ requestProblemInformation })}
              />
            </div>
          </FormSectionBlock>

          <FormSectionBlock
            eyebrow="USER PROPERTIES"
            title="连接用户属性"
            description="随 CONNECT 报文发送给 Broker。"
          >
            <UserPropertiesEditor
              label="连接用户属性"
              properties={form.userProperties ?? []}
              onChange={(userProperties) => patch({ userProperties })}
            />
          </FormSectionBlock>
        </div>
      ) : null}

      {visibleSection === 'will' ? (
        <div className="mqtt-form-page" role="tabpanel">
          <FormSectionBlock
            eyebrow="LAST WILL"
            title="遗嘱消息"
            description="Runtime 异常断线时由 Broker 代为发布，用于上报离线状态。"
          >
            <ToggleField
              checked={Boolean(form.lastWill)}
              description="启用后必须填写遗嘱 Topic。"
              label="启用遗嘱消息"
              onChange={(enabled) => patch({ lastWill: enabled ? { ...emptyLastWill } : undefined })}
            />
          </FormSectionBlock>

          {form.lastWill ? (
            <>
              <FormSectionBlock eyebrow="MESSAGE" title="消息内容">
                <div className="mqtt-field-grid">
                  <Field
                    label="Will Topic"
                    placeholder="edge/{edge_id}/status"
                    value={form.lastWill.topic}
                    onChange={(topic) => patchLastWill(form, patch, { topic })}
                  />
                  <label className="editor-control mqtt-qos-control">
                    <span>Will QoS</span>
                    <select
                      aria-label="Will QoS"
                      value={form.lastWill.qos}
                      onChange={(event) => patchLastWill(form, patch, { qos: Number(event.target.value) })}
                    >
                      <option value={0}>QoS 0</option>
                      <option value={1}>QoS 1</option>
                      <option value={2}>QoS 2</option>
                    </select>
                  </label>
                  <label className="editor-control mqtt-full-field">
                    <span>Will Payload</span>
                    <textarea
                      aria-label="Will Payload"
                      placeholder='{"status":"offline"}'
                      rows={4}
                      value={form.lastWill.payload}
                      onChange={(event) => patchLastWill(form, patch, { payload: event.target.value })}
                    />
                  </label>
                </div>
                <ToggleField
                  checked={form.lastWill.retain}
                  description="Broker 将遗嘱保存为该 Topic 的最新保留消息。"
                  label="Will Retain"
                  onChange={(retain) => patchLastWill(form, patch, { retain })}
                />
              </FormSectionBlock>

              {protocolVersion === '5.0' ? (
                <>
                  <FormSectionBlock
                    eyebrow="MQTT 5 WILL PROPERTIES"
                    title="遗嘱属性"
                    description="这些属性仅写入 MQTT 5.0 Will Properties。"
                  >
                    <div className="mqtt-field-grid">
                      <OptionalNumberField
                        label="Will Delay（秒）"
                        min="0"
                        value={form.lastWill.delayIntervalSeconds}
                        onChange={(delayIntervalSeconds) =>
                          patchLastWill(form, patch, { delayIntervalSeconds })
                        }
                      />
                      <OptionalNumberField
                        label="Message Expiry（秒）"
                        min="0"
                        value={form.lastWill.messageExpiryIntervalSeconds}
                        onChange={(messageExpiryIntervalSeconds) =>
                          patchLastWill(form, patch, { messageExpiryIntervalSeconds })
                        }
                      />
                      <Field
                        label="Content Type（可选）"
                        placeholder="application/json"
                        value={form.lastWill.contentType ?? ''}
                        onChange={(contentType) => patchLastWill(form, patch, { contentType })}
                      />
                      <Field
                        label="Response Topic（可选）"
                        value={form.lastWill.responseTopic ?? ''}
                        onChange={(responseTopic) => patchLastWill(form, patch, { responseTopic })}
                      />
                      <Field
                        label="Correlation Data（可选）"
                        value={form.lastWill.correlationData ?? ''}
                        onChange={(correlationData) => patchLastWill(form, patch, { correlationData })}
                      />
                      <ToggleField
                        checked={form.lastWill.payloadFormatUtf8 ?? true}
                        description="声明 Will Payload 为 UTF-8 文本。"
                        label="Payload Format UTF-8"
                        onChange={(payloadFormatUtf8) =>
                          patchLastWill(form, patch, { payloadFormatUtf8 })
                        }
                      />
                    </div>
                  </FormSectionBlock>
                  <FormSectionBlock eyebrow="USER PROPERTIES" title="遗嘱用户属性">
                    <UserPropertiesEditor
                      label="遗嘱用户属性"
                      properties={form.lastWill.userProperties ?? []}
                      onChange={(userProperties) => patchLastWill(form, patch, { userProperties })}
                    />
                  </FormSectionBlock>
                </>
              ) : null}
            </>
          ) : (
            <div className="mqtt-empty-state">
              <strong>遗嘱消息未启用</strong>
              <span>启用后，Broker 可在 Runtime 非正常断线时自动发布离线状态。</span>
            </div>
          )}
        </div>
      ) : null}
    </div>
  );
}

function FormSectionBlock({
  children,
  description,
  eyebrow,
  title,
}: {
  children: React.ReactNode;
  description?: string;
  eyebrow: string;
  title: string;
}) {
  return (
    <section className="mqtt-form-section">
      <div className="mqtt-section-heading">
        <div>
          <span>{eyebrow}</span>
          <h4>{title}</h4>
        </div>
        {description ? <p>{description}</p> : null}
      </div>
      {children}
    </section>
  );
}

function Field({
  label,
  min,
  onChange,
  placeholder,
  type = 'text',
  value,
}: {
  label: string;
  min?: string;
  onChange: (value: string) => void;
  placeholder?: string;
  type?: string;
  value: string;
}) {
  return (
    <label className="editor-control">
      <span>{label}</span>
      <input
        aria-label={label}
        min={min}
        placeholder={placeholder}
        type={type}
        value={value}
        onChange={(event) => onChange(event.target.value)}
      />
    </label>
  );
}

function OptionalNumberField({
  label,
  min,
  onChange,
  value,
}: {
  label: string;
  min: string;
  onChange: (value: number | undefined) => void;
  value?: number;
}) {
  return (
    <Field
      label={label}
      min={min}
      type="number"
      value={value === undefined ? '' : String(value)}
      onChange={(next) => onChange(next === '' ? undefined : Number(next))}
    />
  );
}

function ToggleField({
  checked,
  description,
  label,
  onChange,
}: {
  checked: boolean;
  description: string;
  label: string;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="mqtt-toggle-field">
      <span>
        <strong>{label}</strong>
        <small>{description}</small>
      </span>
      <input
        aria-label={label}
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
        type="checkbox"
      />
      <i aria-hidden="true" />
    </label>
  );
}

function UserPropertiesEditor({
  label,
  onChange,
  properties,
}: {
  label: string;
  onChange: (properties: MqttUserProperty[]) => void;
  properties: MqttUserProperty[];
}) {
  return (
    <div aria-label={label} className="mqtt-user-properties">
      {properties.length === 0 ? (
        <span className="mqtt-properties-empty">尚未添加用户属性</span>
      ) : null}
      {properties.map((property, index) => (
        <div className="mqtt-property-row" key={`${index}-${property.key}`}>
          <Field
            label={`属性 ${index + 1} Key`}
            value={property.key}
            onChange={(key) => onChange(replaceProperty(properties, index, { ...property, key }))}
          />
          <Field
            label={`属性 ${index + 1} Value`}
            value={property.value}
            onChange={(value) => onChange(replaceProperty(properties, index, { ...property, value }))}
          />
          <button
            aria-label={`删除属性 ${index + 1}`}
            className="icon-button danger"
            onClick={() => onChange(properties.filter((_, propertyIndex) => propertyIndex !== index))}
            title="删除属性"
            type="button"
          >
            <Trash2 size={15} aria-hidden="true" />
          </button>
        </div>
      ))}
      <button
        className="secondary-button mqtt-add-property"
        onClick={() => onChange([...properties, { key: '', value: '' }])}
        type="button"
      >
        <Plus size={15} aria-hidden="true" />
        添加属性
      </button>
    </div>
  );
}

function replaceProperty(
  properties: MqttUserProperty[],
  index: number,
  property: MqttUserProperty,
) {
  return properties.map((item, propertyIndex) => propertyIndex === index ? property : item);
}

function patchLastWill(
  form: MqttUplinkResponse,
  patch: (value: Partial<MqttUplinkResponse>) => void,
  value: Partial<MqttLastWill>,
) {
  if (!form.lastWill) return;
  patch({ lastWill: { ...form.lastWill, ...value } });
}

function parseBroker(value: string): BrokerParts {
  const match = value.trim().match(/^(mqtts?|ssl|tcp):\/\/([^/:]+)(?::(\d+))?/i);
  const transport = match?.[1]?.toLowerCase() === 'mqtts' || match?.[1]?.toLowerCase() === 'ssl'
    ? 'mqtts'
    : 'mqtt';
  return {
    host: match?.[2] ?? '',
    port: match?.[3] ?? (transport === 'mqtts' ? '8883' : '1883'),
    transport,
  };
}
