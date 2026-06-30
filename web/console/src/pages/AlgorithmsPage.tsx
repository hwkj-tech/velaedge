import { useEffect, useMemo, useState, type Dispatch, type SetStateAction } from 'react';
import { Plus, ShieldCheck, X } from 'lucide-react';

import type {
  AlgorithmDsl,
  AlgorithmKind,
  AlgorithmReportPolicy,
  AlgorithmResponse,
  CreateAlgorithmRequest,
  EdgeNodeResponse,
  ManagementActionResponse,
  SaveAlgorithmRequest,
} from '../api/types';
import { Drawer } from '../components/Drawer';
import { PaginationBar } from '../components/PaginationBar';
import './PointMappingsPage.css';

const fallbackDsl: AlgorithmDsl = {
  inputs: [{ alias: 'p', pointId: 'pressure' }],
  trigger: { type: 'onSample' },
  steps: [{ type: 'changeFilter', source: 'p', threshold: 0.2 }],
  outputs: [{ name: 'p', pointId: 'pressure.reported' }],
  report: { mode: 'OnChange', sink: 'velamq-main' },
};

const fallbackAlgorithms: AlgorithmResponse[] = [
  {
    edgeId: 'edge-dev',
    algorithmId: 'pressure-change-report',
    version: '1.0.0',
    algorithmKind: 'ChangeReport',
    dsl: fallbackDsl,
    runtime: 'Rule',
    kind: '变化上报',
    inputIds: ['pressure'],
    outputIds: ['pressure.reported'],
    inputs: 'pressure',
    outputs: 'pressure.reported',
    execution: '边端本地执行',
    validation: '已通过',
  },
];

const fallbackEdges: EdgeNodeResponse[] = [
  {
    edgeId: 'edge-dev',
    displayName: '研发实验室边端',
    site: '研发/实验室',
    runtimeId: 'runtime-dev',
    status: '健康',
    resources: '18.5% / 42% / 61%',
    heartbeat: '8 秒前',
    capabilities: ['algorithm:dsl'],
  },
];

const algorithmKindOptions: Array<[AlgorithmKind, string]> = [
  ['ChangeReport', '变化上报'],
  ['WindowAggregate', '窗口聚合'],
  ['ExpressionAggregate', '表达式聚合'],
  ['ThresholdRule', '阈值告警'],
  ['DurationRule', '持续条件'],
  ['Deadband', '死区过滤'],
  ['Debounce', '去抖动'],
  ['Statistics', '统计计算'],
];

const reportModes: Array<[AlgorithmReportPolicy['mode'], string]> = [
  ['OnOutput', '每次输出'],
  ['OnChange', '变化上报'],
  ['WindowResult', '窗口结果'],
  ['EventOnly', '仅事件'],
];

export function AlgorithmsPage({
  algorithms = fallbackAlgorithms,
  edges = fallbackEdges,
  embedded = false,
  mode = 'configure',
  onAssessRisk,
  onCreateAlgorithm,
  onSaveAlgorithm,
  selectedEdgeId = edges[0]?.edgeId ?? 'edge-dev',
}: {
  algorithms?: AlgorithmResponse[];
  edges?: EdgeNodeResponse[];
  embedded?: boolean;
  mode?: 'configure' | 'list';
  onAssessRisk?: (
    edgeId: string,
  ) => Promise<ManagementActionResponse> | ManagementActionResponse;
  onCreateAlgorithm?: (
    edgeId: string,
    request: CreateAlgorithmRequest,
  ) => Promise<AlgorithmResponse> | AlgorithmResponse;
  onSaveAlgorithm?: (
    edgeId: string,
    algorithmId: string,
    request: SaveAlgorithmRequest,
  ) => Promise<void> | void;
  selectedEdgeId?: string;
}) {
  const [selectedAlgorithmId, setSelectedAlgorithmId] = useState(
    () => algorithms[0]?.algorithmId ?? fallbackAlgorithms[0].algorithmId,
  );
  const [page, setPage] = useState(1);
  const selectedAlgorithm =
    algorithms.find((algorithm) => algorithm.algorithmId === selectedAlgorithmId) ??
    algorithms[0] ??
    fallbackAlgorithms[0];
  const [form, setForm] = useState(() => algorithmToEditorForm(selectedAlgorithm));
  const [saveState, setSaveState] = useState<'idle' | 'saving' | 'saved' | 'error'>(
    'idle',
  );
  const [toolbarMessage, setToolbarMessage] = useState('');
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [editDialogOpen, setEditDialogOpen] = useState(false);
  const [createForm, setCreateForm] = useState<EditorForm>(() => ({
    algorithmKind: 'ChangeReport',
    expression: 'a + b + c',
    inputPoints: fallbackAlgorithms[0].inputIds.join(', '),
    outputPoint: 'algorithm.output',
    reportMode: 'OnChange',
    sink: 'velamq-main',
    threshold: '0.2',
    version: '1.0.0',
    windowMs: '60000',
  }));
  const [createAlgorithmId, setCreateAlgorithmId] = useState('');
  const [actionState, setActionState] = useState<
    'idle' | 'assessing' | 'creating'
  >('idle');
  const isConfigureMode = mode === 'configure';
  const pageSize = 10;
  const totalPages = Math.max(1, Math.ceil(algorithms.length / pageSize));
  const currentPage = Math.min(page, totalPages);
  const visibleAlgorithms = algorithms.slice(
    (currentPage - 1) * pageSize,
    currentPage * pageSize,
  );
  const activeEdge =
    edges.find((edge) => edge.edgeId === selectedEdgeId) ?? edges[0] ?? fallbackEdges[0];
  const dslPreview = useMemo(() => buildAlgorithmDsl(form), [form]);
  const createDslPreview = useMemo(() => buildAlgorithmDsl(createForm), [createForm]);

  useEffect(() => {
    setForm(algorithmToEditorForm(selectedAlgorithm));
    setSaveState((current) =>
      current === 'saving' || current === 'saved' ? current : 'idle',
    );
  }, [selectedAlgorithm]);

  useEffect(() => {
    if (
      algorithms.length > 0 &&
      !algorithms.some((algorithm) => algorithm.algorithmId === selectedAlgorithmId)
    ) {
      setSelectedAlgorithmId(algorithms[0].algorithmId);
    }
  }, [algorithms, selectedAlgorithmId]);

  useEffect(() => {
    setPage(1);
  }, [selectedEdgeId]);

  const handleSave = async () => {
    setSaveState('saving');

    try {
      await onSaveAlgorithm?.(
        selectedEdgeId,
        selectedAlgorithm.algorithmId,
        formToSaveRequest(form),
      );
      setSaveState('saved');
    } catch {
      setSaveState('error');
    }
  };

  const handleAssessRisk = async () => {
    setActionState('assessing');
    setToolbarMessage('');

    try {
      const result = await onAssessRisk?.(selectedEdgeId);
      setToolbarMessage(
        result?.status ? `算法风险评估 ${result.status}` : '算法风险评估已完成',
      );
    } catch {
      setToolbarMessage('算法风险评估失败');
    } finally {
      setActionState('idle');
    }
  };

  const handleCreateAlgorithm = async () => {
    setActionState('creating');
    setToolbarMessage('');

    try {
      const created = await onCreateAlgorithm?.(selectedEdgeId, {
        ...formToSaveRequest(createForm),
        algorithmId: createAlgorithmId.trim(),
      });
      setToolbarMessage(
        created ? `已创建算法 ${created.algorithmId}` : '已创建算法',
      );
      setCreateDialogOpen(false);
    } catch {
      setToolbarMessage('创建算法失败');
    } finally {
      setActionState('idle');
    }
  };

  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>算法配置</h2>
          <p>
            使用点位驱动 DSL 配置边端数据处理、虚拟点位和 MQTT 上报策略。
          </p>
        </div>
        <div className="toolbar">
          {isConfigureMode && !embedded ? (
            <div className="edge-context-pill" aria-label="当前边端">
              <span>当前边端</span>
              <strong>{activeEdge.displayName} / {activeEdge.edgeId}</strong>
            </div>
          ) : null}
          {toolbarMessage ? (
            <span className="toolbar-status" role="status">
              {toolbarMessage}
            </span>
          ) : null}
          {isConfigureMode ? (
            <>
              <button
                className="secondary-button"
                disabled={actionState === 'assessing'}
                onClick={() => {
                  void handleAssessRisk();
                }}
                type="button"
              >
                <ShieldCheck size={15} aria-hidden="true" />
                {actionState === 'assessing' ? '评估中' : '风险评估'}
              </button>
              <button
                className="primary-button"
                disabled={actionState === 'creating'}
                onClick={() => setCreateDialogOpen(true)}
                type="button"
              >
                <Plus size={15} aria-hidden="true" />
                新建算法
              </button>
            </>
          ) : null}
        </div>
      </section>

      {createDialogOpen ? (
        <AlgorithmDialog
          actionState={actionState}
          algorithmId={createAlgorithmId}
          dsl={createDslPreview}
          form={createForm}
          onAlgorithmIdChange={setCreateAlgorithmId}
          onClose={() => setCreateDialogOpen(false)}
          onSubmit={() => {
            void handleCreateAlgorithm();
          }}
          setForm={setCreateForm}
        />
      ) : null}

      <div className={isConfigureMode ? 'point-config-layout' : 'point-config-layout list-only'}>
        <section className="panel point-table-panel">
          <div className="panel-header">
            <h3>算法模板</h3>
            <span>
              {activeEdge.displayName} · {algorithms.length} 个 DSL 算法
            </span>
          </div>
          <div className="table-wrap">
            <table className="ops-table">
              <thead>
                <tr>
                  <th>Algorithm ID</th>
                  <th>类型</th>
                  <th>输入点位</th>
                  <th>输出点位</th>
                  <th>校验</th>
                </tr>
              </thead>
              <tbody>
                {visibleAlgorithms.map((algorithm) => (
                  <tr key={`${algorithm.edgeId}:${algorithm.algorithmId}`}>
                    <td>
                      {isConfigureMode ? (
                        <button
                          aria-label={`选择算法 ${algorithm.algorithmId}`}
                          aria-pressed={
                            algorithm.algorithmId === selectedAlgorithm.algorithmId
                          }
                          className="point-id-button"
                          onClick={() => {
                            setSelectedAlgorithmId(algorithm.algorithmId);
                            setEditDialogOpen(true);
                          }}
                          type="button"
                        >
                          {algorithm.algorithmId}
                        </button>
                      ) : (
                        algorithm.algorithmId
                      )}
                    </td>
                    <td>{algorithm.kind}</td>
                    <td>{algorithm.inputs}</td>
                    <td>{algorithm.outputs}</td>
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
          {algorithms.length > pageSize ? (
            <PaginationBar
              ariaLabel="算法分页"
              currentPage={currentPage}
              onNext={() => setPage((value) => Math.min(totalPages, value + 1))}
              onPrevious={() => setPage((value) => Math.max(1, value - 1))}
              totalPages={totalPages}
            />
          ) : null}
        </section>

        {isConfigureMode && editDialogOpen ? (
          <Drawer
            onClose={() => setEditDialogOpen(false)}
            subtitle="保存后进入待发布配置，发布后边端 runtime 按点位样本执行"
            title={`编辑算法 ${selectedAlgorithm.algorithmId}`}
            footer={
              <>
                <span className={`editor-status ${saveState}`} role="status">
                  {saveStatusText(saveState)}
                </span>
                <button
                  className="secondary-button"
                  onClick={() => {
                    setForm(algorithmToEditorForm(selectedAlgorithm));
                    setSaveState('idle');
                    setEditDialogOpen(false);
                  }}
                  type="button"
                >
                  取消
                </button>
                <button
                  className="primary-button"
                  disabled={saveState === 'saving'}
                  onClick={handleSave}
                  type="button"
                >
                  {saveState === 'saving' ? '保存中' : '保存'}
                </button>
              </>
            }
          >
            <AlgorithmEditor form={form} setForm={setForm} />
            <DslPreview dsl={dslPreview} />
            <DrawerSection
              fields={[
                ['边端', selectedEdgeId],
                ['Algorithm ID', selectedAlgorithm.algorithmId],
                ['当前版本', selectedAlgorithm.version],
                ['当前类型', selectedAlgorithm.kind],
              ]}
              title="当前版本"
            />
          </Drawer>
        ) : null}
      </div>
    </div>
  );
}

function AlgorithmDialog({
  actionState,
  algorithmId,
  dsl,
  form,
  onAlgorithmIdChange,
  onClose,
  onSubmit,
  setForm,
}: {
  actionState: 'idle' | 'assessing' | 'creating';
  algorithmId: string;
  dsl: AlgorithmDsl;
  form: EditorForm;
  onAlgorithmIdChange: (value: string) => void;
  onClose: () => void;
  onSubmit: () => void;
  setForm: Dispatch<SetStateAction<EditorForm>>;
}) {
  return (
    <div className="modal-backdrop">
      <form
        aria-labelledby="algorithm-create-dialog-title"
        className="modal-panel"
        onSubmit={(event) => {
          event.preventDefault();
          onSubmit();
        }}
        role="dialog"
      >
        <div className="modal-header">
          <h3 id="algorithm-create-dialog-title">新建算法</h3>
          <button
            aria-label="关闭"
            className="icon-button"
            onClick={onClose}
            type="button"
          >
            <X size={16} aria-hidden="true" />
          </button>
        </div>
        <div className="form-grid">
          <label>
            <span>Algorithm ID</span>
            <input
              aria-label="新建 Algorithm ID"
              required
              value={algorithmId}
              onChange={(event) => onAlgorithmIdChange(event.target.value)}
            />
          </label>
        </div>
        <AlgorithmEditor form={form} setForm={setForm} />
        <DslPreview dsl={dsl} />
        <div className="modal-actions">
          <button className="secondary-button" onClick={onClose} type="button">
            取消
          </button>
          <button
            className="primary-button"
            disabled={actionState === 'creating'}
            type="submit"
          >
            {actionState === 'creating' ? '保存中' : '保存'}
          </button>
        </div>
      </form>
    </div>
  );
}

function AlgorithmEditor({
  form,
  setForm,
}: {
  form: EditorForm;
  setForm: Dispatch<SetStateAction<EditorForm>>;
}) {
  return (
    <>
      <section className="drawer-section">
        <h4>算法模板</h4>
        <div className="editor-grid">
          <label className="editor-control">
            <span>算法版本</span>
            <input
              aria-label="算法版本"
              value={form.version}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  version: event.target.value,
                }))
              }
            />
          </label>
          <label className="editor-control">
            <span>算法类型</span>
            <select
              aria-label="算法类型"
              value={form.algorithmKind}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  algorithmKind: event.target.value as AlgorithmKind,
                }))
              }
            >
              {algorithmKindOptions.map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </label>
        </div>
      </section>
      <section className="drawer-section">
        <h4>点位与参数</h4>
        <div className="editor-grid">
          <label className="editor-control">
            <span>输入点位</span>
            <input
              aria-label="输入点位"
              value={form.inputPoints}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  inputPoints: event.target.value,
                }))
              }
            />
          </label>
          <label className="editor-control">
            <span>输出虚拟点位</span>
            <input
              aria-label="输出虚拟点位"
              value={form.outputPoint}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  outputPoint: event.target.value,
                }))
              }
            />
          </label>
          <label className="editor-control">
            <span>变化阈值</span>
            <input
              aria-label="变化阈值"
              value={form.threshold}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  threshold: event.target.value,
                }))
              }
            />
          </label>
          <label className="editor-control">
            <span>窗口大小(ms)</span>
            <input
              aria-label="窗口大小(ms)"
              value={form.windowMs}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  windowMs: event.target.value,
                }))
              }
            />
          </label>
          <label className="editor-control">
            <span>表达式</span>
            <input
              aria-label="表达式"
              value={form.expression}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  expression: event.target.value,
                }))
              }
            />
          </label>
          <label className="editor-control">
            <span>上报模式</span>
            <select
              aria-label="上报模式"
              value={form.reportMode}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  reportMode: event.target.value as AlgorithmReportPolicy['mode'],
                }))
              }
            >
              {reportModes.map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </label>
          <label className="editor-control">
            <span>MQTT Sink</span>
            <input
              aria-label="MQTT Sink"
              value={form.sink}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  sink: event.target.value,
                }))
              }
            />
          </label>
        </div>
      </section>
    </>
  );
}

function DslPreview({ dsl }: { dsl: AlgorithmDsl }) {
  return (
    <section className="drawer-section">
      <h4>DSL 预览</h4>
      <pre className="code-preview" aria-label="DSL 预览">
        {JSON.stringify(dsl, null, 2)}
      </pre>
    </section>
  );
}

interface EditorForm {
  algorithmKind: AlgorithmKind;
  expression: string;
  inputPoints: string;
  outputPoint: string;
  reportMode: AlgorithmReportPolicy['mode'];
  sink: string;
  threshold: string;
  version: string;
  windowMs: string;
}

function algorithmToEditorForm(algorithm: AlgorithmResponse): EditorForm {
  const firstStep = algorithm.dsl.steps[0];
  return {
    algorithmKind: algorithm.algorithmKind || 'ChangeReport',
    expression:
      firstStep?.type === 'expression' ? firstStep.expr : inputAliases(algorithm.dsl).join(' + '),
    inputPoints:
      algorithm.dsl.inputs?.length > 0
        ? algorithm.dsl.inputs.map((input) => input.pointId).join(', ')
        : algorithm.inputs,
    outputPoint:
      algorithm.dsl.outputs?.[0]?.pointId ||
      algorithm.outputIds?.[0] ||
      algorithm.outputs ||
      'algorithm.output',
    reportMode: algorithm.dsl.report?.mode || 'OnChange',
    sink: algorithm.dsl.report?.sink || 'velamq-main',
    threshold:
      firstStep?.type === 'changeFilter' || firstStep?.type === 'thresholdRule'
        ? String(firstStep.threshold)
        : '0.2',
    version: algorithm.version,
    windowMs:
      algorithm.dsl.trigger?.type === 'window'
        ? String(algorithm.dsl.trigger.everyMs)
        : '60000',
  };
}

function formToSaveRequest(form: EditorForm): SaveAlgorithmRequest {
  return {
    algorithmKind: form.algorithmKind,
    dsl: buildAlgorithmDsl(form),
    version: form.version.trim(),
  };
}

function buildAlgorithmDsl(form: EditorForm): AlgorithmDsl {
  const inputPoints = splitCsv(form.inputPoints);
  const inputs = inputPoints.map((pointId, index) => ({
    alias: inputPoints.length === 1 ? 'p' : String.fromCharCode(97 + index),
    pointId,
  }));
  const source = inputs[0]?.alias || 'p';
  const outputName = outputNameFromPoint(form.outputPoint);
  const report = {
    mode: form.reportMode,
    sink: form.sink.trim() || 'velamq-main',
  };

  if (form.algorithmKind === 'WindowAggregate') {
    return {
      inputs,
      trigger: { type: 'window', everyMs: parsePositiveInt(form.windowMs, 60000) },
      steps: [
        {
          type: 'windowAggregate',
          source,
          functions: [{ function: 'avg', output: outputName }],
        },
      ],
      outputs: [{ name: outputName, pointId: form.outputPoint.trim() }],
      report: { ...report, mode: 'WindowResult' },
    };
  }

  if (form.algorithmKind === 'ExpressionAggregate') {
    return {
      inputs,
      trigger: { type: 'onAnyInput' },
      steps: [{ type: 'expression', output: outputName, expr: form.expression.trim() }],
      outputs: [{ name: outputName, pointId: form.outputPoint.trim() }],
      report,
    };
  }

  if (form.algorithmKind === 'ThresholdRule') {
    return {
      inputs,
      trigger: { type: 'onSample' },
      steps: [
        {
          type: 'thresholdRule',
          source,
          operator: 'Gt',
          threshold: parseNumber(form.threshold, 0),
          event: {
            code: `${form.outputPoint.trim() || 'ALGORITHM'}_ALARM`.toUpperCase(),
            severity: 'Warning',
            message: '算法阈值告警',
          },
        },
      ],
      outputs: [{ name: outputName, pointId: form.outputPoint.trim() }],
      report: { ...report, mode: 'EventOnly' },
    };
  }

  return {
    inputs,
    trigger: { type: 'onSample' },
    steps: [{ type: 'changeFilter', source, threshold: parseNumber(form.threshold, 0) }],
    outputs: [{ name: outputName, pointId: form.outputPoint.trim() }],
    report: { ...report, mode: 'OnChange' },
  };
}

function inputAliases(dsl: AlgorithmDsl): string[] {
  return dsl.inputs.map((input) => input.alias);
}

function splitCsv(value: string): string[] {
  return value
    .split(',')
    .map((item) => item.trim())
    .filter(Boolean);
}

function outputNameFromPoint(pointId: string): string {
  const parts = pointId.split('.');
  return parts[parts.length - 1]?.trim() || 'output';
}

function parseNumber(value: string, fallback: number): number {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function parsePositiveInt(value: string, fallback: number): number {
  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

function saveStatusText(saveState: 'idle' | 'saving' | 'saved' | 'error') {
  switch (saveState) {
    case 'saving':
      return '保存中';
    case 'saved':
      return '已保存';
    case 'error':
      return '保存失败';
    case 'idle':
      return '';
  }
}

function DrawerSection({
  fields,
  title,
}: {
  fields: Array<[string, string]>;
  title: string;
}) {
  return (
    <section className="drawer-section">
      <h4>{title}</h4>
      <div className="editor-grid">
        {fields.map(([label, value]) => (
          <div className="editor-field" key={label}>
            <span>{label}</span>
            <strong>{value}</strong>
          </div>
        ))}
      </div>
    </section>
  );
}
