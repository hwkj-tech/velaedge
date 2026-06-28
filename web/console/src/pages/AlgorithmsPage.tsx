import { useEffect, useState } from 'react';
import { Plus, ShieldCheck, X } from 'lucide-react';

import type {
  AlgorithmResponse,
  CreateAlgorithmRequest,
  EdgeNodeResponse,
  ManagementActionResponse,
  SaveAlgorithmRequest,
} from '../api/types';
import { Drawer } from '../components/Drawer';
import './PointMappingsPage.css';

const fallbackAlgorithms: AlgorithmResponse[] = [
  {
    edgeId: 'edge-dev',
    algorithmId: 'pump-anomaly-v1',
    version: '1.0.0',
    runtime: 'Onnx',
    kind: '异常检测',
    inputIds: ['pressure', 'running'],
    outputIds: ['pump.anomaly_score'],
    inputs: 'pressure, running',
    outputs: 'pump.anomaly_score',
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
    capabilities: ['algorithm:onnx'],
  },
];

const runtimeOptions = [
  ['Rule', '规则算法'],
  ['Wasm', 'WASM 算法'],
  ['Onnx', 'ONNX 模型'],
  ['Python', 'Python 算法'],
];

export function AlgorithmsPage({
  algorithms = fallbackAlgorithms,
  edges = fallbackEdges,
  mode = 'configure',
  onAssessRisk,
  onCreateAlgorithm,
  onSaveAlgorithm,
  onSelectEdge,
  selectedEdgeId = edges[0]?.edgeId ?? 'edge-dev',
}: {
  algorithms?: AlgorithmResponse[];
  edges?: EdgeNodeResponse[];
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
  onSelectEdge?: (edgeId: string) => Promise<void> | void;
  selectedEdgeId?: string;
}) {
  const [selectedAlgorithmId, setSelectedAlgorithmId] = useState(
    () => algorithms[0]?.algorithmId ?? fallbackAlgorithms[0].algorithmId,
  );
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
  const [createForm, setCreateForm] = useState({
    algorithmId: '',
    inputIds: fallbackAlgorithms[0].inputIds.join(', '),
    outputIds: 'algorithm.output',
    runtime: 'Rule',
    version: '1.0.0',
  });
  const [actionState, setActionState] = useState<
    'idle' | 'assessing' | 'creating'
  >('idle');
  const isConfigureMode = mode === 'configure';
  const activeEdge =
    edges.find((edge) => edge.edgeId === selectedEdgeId) ?? edges[0] ?? fallbackEdges[0];

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

  const handleSelectEdge = async (edgeId: string) => {
    setSaveState('idle');
    setToolbarMessage('');
    await onSelectEdge?.(edgeId);
  };

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
        algorithmId: createForm.algorithmId.trim(),
        inputIds: splitCsv(createForm.inputIds),
        outputIds: splitCsv(createForm.outputIds),
        runtime: createForm.runtime,
        version: createForm.version.trim(),
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
            管理通用边缘算法模板、输入点位和本地执行策略。Agent 可生成草稿，但发布前必须人工复核。
          </p>
        </div>
        <div className="toolbar">
          {isConfigureMode ? (
            <label className="release-edge-select">
              <span>配置边端</span>
              <select
                aria-label="配置边端"
                value={selectedEdgeId}
                onChange={(event) => {
                  void handleSelectEdge(event.target.value);
                }}
              >
                {edges.map((edge) => (
                  <option key={edge.edgeId} value={edge.edgeId}>
                    {edge.displayName} / {edge.edgeId}
                  </option>
                ))}
              </select>
            </label>
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
        <div className="modal-backdrop">
          <form
            aria-labelledby="algorithm-create-dialog-title"
            className="modal-panel"
            onSubmit={(event) => {
              event.preventDefault();
              void handleCreateAlgorithm();
            }}
            role="dialog"
          >
            <div className="modal-header">
              <h3 id="algorithm-create-dialog-title">新建算法</h3>
              <button
                aria-label="关闭"
                className="icon-button"
                onClick={() => setCreateDialogOpen(false)}
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
                  value={createForm.algorithmId}
                  onChange={(event) =>
                    setCreateForm((current) => ({
                      ...current,
                      algorithmId: event.target.value,
                    }))
                  }
                />
              </label>
              <label>
                <span>算法版本</span>
                <input
                  aria-label="新建算法版本"
                  required
                  value={createForm.version}
                  onChange={(event) =>
                    setCreateForm((current) => ({
                      ...current,
                      version: event.target.value,
                    }))
                  }
                />
              </label>
              <label>
                <span>算法运行时</span>
                <select
                  aria-label="新建算法运行时"
                  value={createForm.runtime}
                  onChange={(event) =>
                    setCreateForm((current) => ({
                      ...current,
                      runtime: event.target.value,
                    }))
                  }
                >
                  {runtimeOptions.map(([value, label]) => (
                    <option key={value} value={value}>
                      {label}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                <span>输入点位</span>
                <input
                  aria-label="新建算法输入点位"
                  required
                  value={createForm.inputIds}
                  onChange={(event) =>
                    setCreateForm((current) => ({
                      ...current,
                      inputIds: event.target.value,
                    }))
                  }
                />
              </label>
              <label>
                <span>输出变量</span>
                <input
                  aria-label="新建算法输出变量"
                  required
                  value={createForm.outputIds}
                  onChange={(event) =>
                    setCreateForm((current) => ({
                      ...current,
                      outputIds: event.target.value,
                    }))
                  }
                />
              </label>
            </div>
            <div className="modal-actions">
              <button
                className="secondary-button"
                onClick={() => setCreateDialogOpen(false)}
                type="button"
              >
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
      ) : null}

      <div className={isConfigureMode ? 'point-config-layout' : 'point-config-layout list-only'}>
        <section className="panel point-table-panel">
          <div className="panel-header">
            <h3>算法模板</h3>
            <span>
              {activeEdge.displayName} · {algorithms.length} 个本地算法
            </span>
          </div>
          <div className="table-wrap">
            <table className="ops-table">
              <thead>
                <tr>
                  <th>Algorithm ID</th>
                  <th>类型</th>
                  <th>输入</th>
                  <th>输出</th>
                  <th>校验</th>
                </tr>
              </thead>
              <tbody>
                {algorithms.map((algorithm) => (
                  <tr key={`${algorithm.edgeId}:${algorithm.algorithmId}`}>
                    <td>
                      {isConfigureMode ? (
                        <button
                          aria-label={`选择算法 ${algorithm.algorithmId}`}
                          aria-pressed={
                            algorithm.algorithmId === selectedAlgorithm.algorithmId
                          }
                          className="point-id-button"
                          onClick={() => setSelectedAlgorithmId(algorithm.algorithmId)}
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
        </section>

        {isConfigureMode ? (
          <Drawer
          subtitle="云端草稿，发布后边端 runtime 加载或更新本地算法"
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
          <section className="drawer-section">
            <h4>算法参数</h4>
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
                <span>算法运行时</span>
                <select
                  aria-label="算法运行时"
                  value={form.runtime}
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      runtime: event.target.value,
                    }))
                  }
                >
                  {runtimeOptions.map(([value, label]) => (
                    <option key={value} value={value}>
                      {label}
                    </option>
                  ))}
                </select>
              </label>
            </div>
          </section>
          <section className="drawer-section">
            <h4>输入输出</h4>
            <div className="editor-grid">
              <label className="editor-control">
                <span>输入点位</span>
                <input
                  aria-label="输入点位"
                  value={form.inputIds}
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      inputIds: event.target.value,
                    }))
                  }
                />
              </label>
              <label className="editor-control">
                <span>输出变量</span>
                <input
                  aria-label="输出变量"
                  value={form.outputIds}
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      outputIds: event.target.value,
                    }))
                  }
                />
              </label>
            </div>
          </section>
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

interface EditorForm {
  inputIds: string;
  outputIds: string;
  runtime: string;
  version: string;
}

function algorithmToEditorForm(algorithm: AlgorithmResponse): EditorForm {
  return {
    inputIds: algorithm.inputIds?.length > 0 ? algorithm.inputIds.join(', ') : algorithm.inputs,
    outputIds:
      algorithm.outputIds?.length > 0 ? algorithm.outputIds.join(', ') : algorithm.outputs,
    runtime: algorithm.runtime || inferRuntime(algorithm.kind),
    version: algorithm.version,
  };
}

function formToSaveRequest(form: EditorForm): SaveAlgorithmRequest {
  return {
    version: form.version.trim(),
    runtime: form.runtime,
    inputIds: splitCsv(form.inputIds),
    outputIds: splitCsv(form.outputIds),
  };
}

function splitCsv(value: string): string[] {
  return value
    .split(',')
    .map((item) => item.trim())
    .filter(Boolean);
}

function inferRuntime(kind: string): string {
  if (kind.includes('WASM')) {
    return 'Wasm';
  }
  if (kind.includes('Python')) {
    return 'Python';
  }
  if (kind.includes('规则')) {
    return 'Rule';
  }
  return 'Onnx';
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
