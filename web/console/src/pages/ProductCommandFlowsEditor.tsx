import { useMemo, useState } from 'react';
import { Copy, Edit3, Plus, Trash2, X } from 'lucide-react';

import type { CommandFlowConfig, CommandGraphNode } from '../api/types';
import { Modal } from '../components/Modal';
import './ProductCommandFlowsEditor.css';

interface ProductCommandPoint {
  access: 'read' | 'read_write' | 'write';
  pointId: string;
  semanticId: string;
}

interface FlowEditorState {
  allowedSources: string;
  enabled: boolean;
  flowId: string;
  maxCommands: number;
  name: string;
  qos: number;
  replyTopicTemplate: string;
  requireConfirmation: boolean;
  selectedPointIds: string[];
  subscribeTopic: string;
  valuePaths: Record<string, string>;
  verification: 'response' | 'readback';
  windowMs: number;
}

export function ProductCommandFlowsEditor({
  flows,
  mqttConnectionId,
  onChange,
  points,
  protocolConnectionId,
}: {
  flows: CommandFlowConfig[];
  mqttConnectionId: string;
  onChange: (flows: CommandFlowConfig[]) => void;
  points: ProductCommandPoint[];
  protocolConnectionId?: string;
}) {
  const writablePoints = useMemo(
    () => points.filter((point) => point.access !== 'read'),
    [points],
  );
  const [editingFlowId, setEditingFlowId] = useState<string>();
  const [editor, setEditor] = useState<FlowEditorState>();
  const [deleteTarget, setDeleteTarget] = useState<CommandFlowConfig>();
  const [message, setMessage] = useState('');

  const openCreate = () => {
    const sequence = nextFlowSequence(flows);
    setEditingFlowId(undefined);
    setMessage('');
    setEditor({
      allowedSources: 'scada, operator',
      enabled: true,
      flowId: `command_flow_${sequence}`,
      maxCommands: 30,
      name: `指令流程 ${sequence}`,
      qos: 1,
      replyTopicTemplate: 'factory/{edge_id}/reply/{command_id}',
      requireConfirmation: false,
      selectedPointIds: [],
      subscribeTopic: 'factory/{edge_id}/command',
      valuePaths: {},
      verification: 'readback',
      windowMs: 60000,
    });
  };

  const openEdit = (flow: CommandFlowConfig) => {
    setEditingFlowId(flow.flow_id);
    setMessage('');
    setEditor(flowToEditor(flow));
  };

  const submit = () => {
    if (!editor) return;
    const error = validateEditor(editor, flows, editingFlowId, writablePoints);
    if (error) {
      setMessage(error);
      return;
    }
    const flow = editorToFlow(
      editor,
      mqttConnectionId,
      writablePoints,
      protocolConnectionId,
    );
    onChange(
      editingFlowId
        ? flows.map((candidate) =>
            candidate.flow_id === editingFlowId ? flow : candidate,
          )
        : [...flows, flow],
    );
    setEditor(undefined);
    setEditingFlowId(undefined);
    setMessage('');
  };

  const duplicate = (flow: CommandFlowConfig) => {
    const sequence = nextFlowSequence(flows);
    const copyFlow = {
      ...flow,
      flow_id: `${flow.flow_id}_copy_${sequence}`,
      name: `${flow.name} 副本`,
      nodes: flow.nodes.map((node) => ({ ...node, params: { ...node.params } })),
      edges: flow.edges.map((edge) => ({ ...edge })),
    };
    onChange([...flows, copyFlow]);
  };

  return (
    <div className="command-flow-manager">
      <div className="command-flow-toolbar">
        <div>
          <strong>{flows.length} 条指令流程</strong>
          <span>{writablePoints.length} 个可写点位</span>
        </div>
        <button
          className="primary-button compact"
          disabled={writablePoints.length === 0}
          onClick={openCreate}
          title={writablePoints.length === 0 ? '请先在点位集中将控制点设为读写或只写' : undefined}
          type="button"
        >
          <Plus aria-hidden="true" size={15} />
          新建指令流程
        </button>
      </div>

      {writablePoints.length === 0 ? (
        <div className="command-flow-empty" role="status">
          当前产品没有可写点位。请先在点位管理中把保持寄存器或线圈配置为“读写/只写”，再回到这里编排下行指令。
        </div>
      ) : null}

      <div className="table-wrap command-flow-table">
        <table className="ops-table">
          <thead>
            <tr>
              <th>流程</th>
              <th>订阅 Topic</th>
              <th>写入点位</th>
              <th>安全策略</th>
              <th>状态</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {flows.length === 0 ? (
              <tr>
                <td className="table-empty" colSpan={6}>尚未配置下行指令流程。</td>
              </tr>
            ) : null}
            {flows.map((flow) => {
              const writeNodes = flow.nodes.filter((node) => node.kind === 'point_write');
              const safety = flow.nodes.find((node) => node.kind === 'safety_gate');
              return (
                <tr key={flow.flow_id}>
                  <td><button className="point-id-button" onClick={() => openEdit(flow)} type="button">{flow.name}</button><small>{flow.flow_id}</small></td>
                  <td><code>{flow.subscribe_topic}</code></td>
                  <td>{writeNodes.map((node) => node.ref_id).filter(Boolean).join(', ') || '-'}</td>
                  <td>{safetySummary(safety)}</td>
                  <td><span className={`tag ${flow.enabled ? 'ok' : ''}`}>{flow.enabled ? '启用' : '停用'}</span></td>
                  <td>
                    <div className="row-actions">
                      <button aria-label={`修改指令流程 ${flow.name}`} className="secondary-button compact" onClick={() => openEdit(flow)} type="button"><Edit3 aria-hidden="true" size={14} />修改</button>
                      <button aria-label={`复制指令流程 ${flow.name}`} className="secondary-button compact icon-only" onClick={() => duplicate(flow)} title="复制" type="button"><Copy aria-hidden="true" size={14} /></button>
                      <button aria-label={`删除指令流程 ${flow.name}`} className="danger-button compact icon-only" onClick={() => setDeleteTarget(flow)} title="删除" type="button"><Trash2 aria-hidden="true" size={14} /></button>
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      {editor ? (
        <Modal onClose={() => setEditor(undefined)}>
          <section aria-labelledby="command-flow-editor-title" className="modal-panel command-flow-editor-modal" role="dialog">
            <div className="modal-header">
              <div>
                <h3 id="command-flow-editor-title">{editingFlowId ? '修改指令流程' : '新建指令流程'}</h3>
                <p>MQTT 输入经安全策略后，只能写入产品声明为可写的点位。</p>
              </div>
              <button aria-label="关闭" className="icon-button" onClick={() => setEditor(undefined)} type="button"><X aria-hidden="true" size={16} /></button>
            </div>

            <div className="command-flow-editor-body">
              <section className="command-editor-section">
                <div className="command-editor-section-title"><span>1</span><div><h4>输入与回执</h4><small>定义 Runtime 订阅和执行结果返回主题</small></div></div>
                <div className="form-grid">
                  <label><span>流程 ID</span><input aria-label="指令流程 ID" disabled={Boolean(editingFlowId)} value={editor.flowId} onChange={(event) => setEditor({ ...editor, flowId: event.target.value })} /></label>
                  <label><span>名称</span><input aria-label="指令流程名称" value={editor.name} onChange={(event) => setEditor({ ...editor, name: event.target.value })} /></label>
                  <label className="span-two"><span>订阅 Topic</span><input aria-label="指令订阅 Topic" value={editor.subscribeTopic} onChange={(event) => setEditor({ ...editor, subscribeTopic: event.target.value })} /></label>
                  <label><span>QoS</span><select aria-label="指令 QoS" value={editor.qos} onChange={(event) => setEditor({ ...editor, qos: Number(event.target.value) })}><option value={0}>0</option><option value={1}>1</option><option value={2}>2</option></select></label>
                  <label><span>回执 Topic</span><input aria-label="指令回执 Topic" value={editor.replyTopicTemplate} onChange={(event) => setEditor({ ...editor, replyTopicTemplate: event.target.value })} /></label>
                </div>
              </section>

              <section className="command-editor-section">
                <div className="command-editor-section-title"><span>2</span><div><h4>安全策略</h4><small>来源白名单、确认令牌和滚动限流在写设备前执行</small></div></div>
                <div className="form-grid">
                  <label className="span-two"><span>允许来源（逗号分隔）</span><input aria-label="允许指令来源" placeholder="scada, operator" value={editor.allowedSources} onChange={(event) => setEditor({ ...editor, allowedSources: event.target.value })} /></label>
                  <label><span>窗口内最大指令数</span><input aria-label="最大指令数" min="1" type="number" value={editor.maxCommands} onChange={(event) => setEditor({ ...editor, maxCommands: Number(event.target.value) })} /></label>
                  <label><span>限流窗口(ms)</span><input aria-label="指令限流窗口" min="1" type="number" value={editor.windowMs} onChange={(event) => setEditor({ ...editor, windowMs: Number(event.target.value) })} /></label>
                  <label className="command-switch"><input aria-label="要求确认令牌" checked={editor.requireConfirmation} onChange={(event) => setEditor({ ...editor, requireConfirmation: event.target.checked })} type="checkbox" /><span>要求 confirmationToken</span></label>
                  <label className="command-switch"><input aria-label="启用指令流程" checked={editor.enabled} onChange={(event) => setEditor({ ...editor, enabled: event.target.checked })} type="checkbox" /><span>启用流程</span></label>
                </div>
              </section>

              <section className="command-editor-section">
                <div className="command-editor-section-title"><span>3</span><div><h4>写入点位</h4><small>选择可写点位并映射 MQTT JSON 消息字段</small></div></div>
                <div className="command-point-picker">
                  {writablePoints.map((point) => (
                    <label key={point.pointId}>
                      <input
                        aria-label={`选择写入点位 ${point.pointId}`}
                        checked={editor.selectedPointIds.includes(point.pointId)}
                        onChange={(event) => {
                          const checked = event.target.checked;
                          setEditor({
                            ...editor,
                            selectedPointIds: checked
                              ? [...editor.selectedPointIds, point.pointId]
                              : editor.selectedPointIds.filter((pointId) => pointId !== point.pointId),
                            valuePaths: checked
                              ? {
                                  ...editor.valuePaths,
                                  [point.pointId]: editor.valuePaths[point.pointId] ?? defaultValuePath(point.pointId),
                                }
                              : editor.valuePaths,
                          });
                        }}
                        type="checkbox"
                      />
                      <span><strong>{point.pointId}</strong><small>{point.semanticId}</small></span>
                      <em>{point.access === 'write' ? '只写' : '读写'}</em>
                    </label>
                  ))}
                </div>
                {editor.selectedPointIds.length > 0 ? (
                  <div className="command-value-mappings">
                    <div className="command-value-mappings-header">
                      <strong>消息字段映射</strong>
                      <code>MQTT JSON → 可写点位</code>
                    </div>
                    {editor.selectedPointIds.map((pointId) => {
                      const point = writablePoints.find((candidate) => candidate.pointId === pointId);
                      if (!point) return null;
                      return (
                        <label key={pointId}>
                          <span>
                            <strong>{point.pointId}</strong>
                            <small>{point.semanticId}</small>
                          </span>
                          <input
                            aria-label={`消息字段路径 ${point.pointId}`}
                            onChange={(event) => setEditor({
                              ...editor,
                              valuePaths: { ...editor.valuePaths, [point.pointId]: event.target.value },
                            })}
                            placeholder={defaultValuePath(point.pointId)}
                            value={editor.valuePaths[point.pointId] ?? defaultValuePath(point.pointId)}
                          />
                          <em>写入</em>
                        </label>
                      );
                    })}
                  </div>
                ) : null}
                <label className="command-verification"><span>写入校验</span><select aria-label="写入校验" value={editor.verification} onChange={(event) => setEditor({ ...editor, verification: event.target.value as FlowEditorState['verification'] })}><option value="response">协议响应</option><option value="readback">写后回读</option></select><small>只写点位会自动使用协议响应校验。</small></label>
              </section>

              {message ? <div className="form-validation-panel" role="alert"><strong>无法保存指令流程</strong><span>{message}</span></div> : null}
            </div>
            <div className="modal-actions">
              <span>保存产品后生成新的配置版本</span>
              <button className="secondary-button" onClick={() => setEditor(undefined)} type="button">取消</button>
              <button className="primary-button" onClick={submit} type="button">保存流程</button>
            </div>
          </section>
        </Modal>
      ) : null}

      {deleteTarget ? (
        <Modal onClose={() => setDeleteTarget(undefined)}>
          <section aria-labelledby="delete-command-flow-title" className="modal-panel compact-modal" role="dialog">
            <div className="modal-header"><div><h3 id="delete-command-flow-title">删除指令流程</h3><p>{deleteTarget.name} 将从下一产品版本中移除。</p></div><button aria-label="关闭" className="icon-button" onClick={() => setDeleteTarget(undefined)} type="button"><X aria-hidden="true" size={16} /></button></div>
            <div className="modal-actions"><button className="secondary-button" onClick={() => setDeleteTarget(undefined)} type="button">取消</button><button className="danger-button" onClick={() => { onChange(flows.filter((flow) => flow.flow_id !== deleteTarget.flow_id)); setDeleteTarget(undefined); }} type="button">确认删除</button></div>
          </section>
        </Modal>
      ) : null}
    </div>
  );
}

function editorToFlow(
  editor: FlowEditorState,
  mqttConnectionId: string,
  writablePoints: ProductCommandPoint[],
  protocolConnectionId?: string,
): CommandFlowConfig {
  const allowedSources = editor.allowedSources.split(',').map((value) => value.trim()).filter(Boolean);
  const safetyParams: Record<string, unknown> = {
    max_commands: editor.maxCommands,
    require_confirmation: editor.requireConfirmation,
    source_path: 'requestedBy',
    window_ms: editor.windowMs,
  };
  if (allowedSources.length > 0) safetyParams.allowed_sources = allowedSources;

  const nodes: CommandGraphNode[] = [
    node('input', 'mqtt_input', 'MQTT 指令输入', 60, 150),
    node('parse', 'json_parse', '解析 JSON', 270, 150),
    { ...node('safety', 'safety_gate', '安全策略', 480, 150), params: safetyParams },
  ];
  const edges: CommandFlowConfig['edges'] = [
    edge('input-parse', 'input', 'parse'),
    edge('parse-safety', 'parse', 'safety'),
  ];
  const selectedPoints = editor.selectedPointIds
    .map((pointId) => writablePoints.find((point) => point.pointId === pointId))
    .filter((point): point is ProductCommandPoint => Boolean(point));

  selectedPoints.forEach((point, index) => {
    const nodeId = `write-${safeNodeId(point.pointId)}`;
    nodes.push({
      ...node(nodeId, 'point_write', `写入 ${point.pointId}`, 700, 70 + index * 120),
      params: {
        value_path: editor.valuePaths[point.pointId]?.trim() || defaultValuePath(point.pointId),
        verification: point.access === 'write' ? 'response' : editor.verification,
      },
      ref_id: point.pointId,
    });
    edges.push(edge(`safety-${nodeId}`, 'safety', nodeId));
  });
  nodes.push(node('reply', 'mqtt_reply', 'MQTT 执行回执', 950, 150));
  selectedPoints.forEach((point) => {
    const nodeId = `write-${safeNodeId(point.pointId)}`;
    edges.push(edge(`${nodeId}-reply`, nodeId, 'reply'));
  });

  return {
    edges,
    enabled: editor.enabled,
    flow_id: editor.flowId.trim(),
    mqtt_connection_id: mqttConnectionId,
    name: editor.name.trim(),
    nodes,
    ...(protocolConnectionId ? { protocol_connection_id: protocolConnectionId } : {}),
    qos: editor.qos,
    reply_topic_template: editor.replyTopicTemplate.trim(),
    subscribe_topic: editor.subscribeTopic.trim(),
  };
}

function flowToEditor(flow: CommandFlowConfig): FlowEditorState {
  const safety = flow.nodes.find((node) => node.kind === 'safety_gate');
  const firstWrite = flow.nodes.find((node) => node.kind === 'point_write');
  const allowedSources = Array.isArray(safety?.params.allowed_sources)
    ? safety.params.allowed_sources.filter((value): value is string => typeof value === 'string').join(', ')
    : '';
  const writeNodes = flow.nodes.filter((node) => node.kind === 'point_write' && node.ref_id);
  const valuePaths = Object.fromEntries(writeNodes.map((node) => [
    node.ref_id as string,
    typeof node.params.value_path === 'string' && node.params.value_path.trim()
      ? node.params.value_path
      : defaultValuePath(node.ref_id as string),
  ]));
  return {
    allowedSources,
    enabled: flow.enabled,
    flowId: flow.flow_id,
    maxCommands: numberParam(safety?.params.max_commands, 30),
    name: flow.name,
    qos: flow.qos,
    replyTopicTemplate: flow.reply_topic_template,
    requireConfirmation: safety?.params.require_confirmation === true,
    selectedPointIds: writeNodes.map((node) => node.ref_id as string),
    subscribeTopic: flow.subscribe_topic,
    valuePaths,
    verification: firstWrite?.params.verification === 'readback' ? 'readback' : 'response',
    windowMs: numberParam(safety?.params.window_ms, 60000),
  };
}

function validateEditor(
  editor: FlowEditorState,
  flows: CommandFlowConfig[],
  editingFlowId: string | undefined,
  writablePoints: ProductCommandPoint[],
): string | undefined {
  if (!editor.flowId.trim() || !/^[A-Za-z0-9_.-]+$/.test(editor.flowId)) return '流程 ID 只能包含字母、数字、点、下划线和横线。';
  if (flows.some((flow) => flow.flow_id === editor.flowId && flow.flow_id !== editingFlowId)) return `流程 ID ${editor.flowId} 已存在。`;
  if (!editor.name.trim()) return '请填写流程名称。';
  if (!validMqttTopic(editor.subscribeTopic, true)) return '订阅 Topic 不能为空，且不能包含非法通配符或空层级。';
  if (!validMqttTopic(editor.replyTopicTemplate, false)) return '回执 Topic 不能为空，且不能使用 MQTT 通配符。';
  if (![0, 1, 2].includes(editor.qos)) return 'QoS 只能是 0、1 或 2。';
  if (!Number.isInteger(editor.maxCommands) || editor.maxCommands < 1) return '最大指令数必须是正整数。';
  if (!Number.isInteger(editor.windowMs) || editor.windowMs < 1) return '限流窗口必须是正整数。';
  if (editor.selectedPointIds.length === 0) return '请至少选择一个可写点位。';
  if (editor.selectedPointIds.some((pointId) => !writablePoints.some((point) => point.pointId === pointId))) return '流程包含已失效或只读的点位，请重新选择。';
  const invalidPathPoint = editor.selectedPointIds.find((pointId) => !validJsonPath(editor.valuePaths[pointId] ?? defaultValuePath(pointId)));
  if (invalidPathPoint) return `点位 ${invalidPathPoint} 的消息字段路径无效，请使用由点分隔的 JSON 字段名。`;
  return undefined;
}

function validJsonPath(path: string): boolean {
  const value = path.trim();
  return value.length > 0
    && value.length <= 256
    && value.split('.').every((segment) => segment.length > 0 && !/\s/.test(segment));
}

function validMqttTopic(topic: string, allowWildcard: boolean): boolean {
  const value = topic.trim();
  if (!value || value.includes('\0') || value.includes('//')) return false;
  if (!allowWildcard && (value.includes('#') || value.includes('+'))) return false;
  if (value.includes('#') && !value.endsWith('#')) return false;
  return value.split('/').every((level) => !level.includes('#') || level === '#');
}

function safetySummary(nodeValue: CommandGraphNode | undefined): string {
  if (!nodeValue) return '未配置';
  const sources = Array.isArray(nodeValue.params.allowed_sources) ? nodeValue.params.allowed_sources.length : 0;
  const max = numberParam(nodeValue.params.max_commands, 0);
  return `${sources ? `${sources} 来源 · ` : ''}${max ? `${max} 次/窗口` : '基础校验'}`;
}

function node(nodeId: string, kind: CommandGraphNode['kind'], label: string, x: number, y: number): CommandGraphNode {
  return { kind, label, node_id: nodeId, params: {}, x, y };
}

function edge(edgeId: string, from: string, to: string): CommandFlowConfig['edges'][number] {
  return { edge_id: edgeId, from, to };
}

function safeNodeId(value: string): string {
  return value.replace(/[^A-Za-z0-9_.-]/g, '-');
}

function defaultValuePath(pointId: string): string {
  return `values.${pointId}`;
}

function numberParam(value: unknown, fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
}

function nextFlowSequence(flows: CommandFlowConfig[]): number {
  let sequence = flows.length + 1;
  while (flows.some((flow) => flow.flow_id === `command_flow_${sequence}`)) sequence += 1;
  return sequence;
}
