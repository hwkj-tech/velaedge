import { useEffect, useState } from 'react';
import { Plus, TimerReset, X } from 'lucide-react';

import type {
  CollectionTaskResponse,
  CreateCollectionTaskRequest,
  EdgeNodeResponse,
  ManagementActionResponse,
  SaveCollectionTaskRequest,
} from '../api/types';
import { Drawer } from '../components/Drawer';
import { PaginationBar } from '../components/PaginationBar';
import './PointMappingsPage.css';

const fallbackTasks: CollectionTaskResponse[] = [
  {
    edgeId: 'edge-dev',
    taskId: 'pump-main',
    deviceId: 'pump-1',
    pointIds: ['pressure', 'running'],
    pointList: 'pressure, running',
    intervalMs: 1000,
    interval: '1000ms',
    enabled: true,
    status: '启用',
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
    capabilities: ['protocol:modbus-tcp'],
  },
];

export function CollectionTasksPage({
  edges = fallbackEdges,
  mode = 'configure',
  onCreateTask,
  onGenerateSchedule,
  onSaveTask,
  onSelectEdge,
  selectedEdgeId = edges[0]?.edgeId ?? 'edge-dev',
  tasks = fallbackTasks,
}: {
  edges?: EdgeNodeResponse[];
  mode?: 'configure' | 'list';
  onCreateTask?: (
    edgeId: string,
    request: CreateCollectionTaskRequest,
  ) => Promise<CollectionTaskResponse> | CollectionTaskResponse;
  onGenerateSchedule?: (
    edgeId: string,
  ) => Promise<ManagementActionResponse> | ManagementActionResponse;
  onSaveTask?: (
    edgeId: string,
    taskId: string,
    request: SaveCollectionTaskRequest,
  ) => Promise<void> | void;
  onSelectEdge?: (edgeId: string) => Promise<void> | void;
  selectedEdgeId?: string;
  tasks?: CollectionTaskResponse[];
}) {
  const [selectedTaskId, setSelectedTaskId] = useState(
    () => tasks[0]?.taskId ?? fallbackTasks[0].taskId,
  );
  const [page, setPage] = useState(1);
  const selectedTask =
    tasks.find((task) => task.taskId === selectedTaskId) ?? tasks[0] ?? fallbackTasks[0];
  const [form, setForm] = useState(() => taskToEditorForm(selectedTask));
  const [saveState, setSaveState] = useState<'idle' | 'saving' | 'saved' | 'error'>(
    'idle',
  );
  const [toolbarMessage, setToolbarMessage] = useState('');
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [createForm, setCreateForm] = useState({
    deviceId: fallbackTasks[0].deviceId,
    enabled: true,
    intervalMs: '1000',
    pointIds: fallbackTasks[0].pointIds.join(', '),
    taskId: '',
  });
  const [actionState, setActionState] = useState<
    'idle' | 'scheduling' | 'creating'
  >('idle');
  const isConfigureMode = mode === 'configure';
  const pageSize = 10;
  const totalPages = Math.max(1, Math.ceil(tasks.length / pageSize));
  const currentPage = Math.min(page, totalPages);
  const visibleTasks = tasks.slice((currentPage - 1) * pageSize, currentPage * pageSize);
  const activeEdge =
    edges.find((edge) => edge.edgeId === selectedEdgeId) ?? edges[0] ?? fallbackEdges[0];

  useEffect(() => {
    setForm(taskToEditorForm(selectedTask));
    setSaveState((current) =>
      current === 'saving' || current === 'saved' ? current : 'idle',
    );
  }, [selectedTask]);

  useEffect(() => {
    if (tasks.length > 0 && !tasks.some((task) => task.taskId === selectedTaskId)) {
      setSelectedTaskId(tasks[0].taskId);
    }
  }, [tasks, selectedTaskId]);

  useEffect(() => {
    setPage(1);
  }, [selectedEdgeId]);

  const handleSelectEdge = async (edgeId: string) => {
    setSaveState('idle');
    setToolbarMessage('');
    await onSelectEdge?.(edgeId);
  };

  const handleSave = async () => {
    const request = formToSaveRequest(form);
    setSaveState('saving');

    try {
      await onSaveTask?.(selectedEdgeId, selectedTask.taskId, request);
      setSaveState('saved');
    } catch {
      setSaveState('error');
    }
  };

  const handleGenerateSchedule = async () => {
    setActionState('scheduling');
    setToolbarMessage('');

    try {
      const result = await onGenerateSchedule?.(selectedEdgeId);
      setToolbarMessage(result?.message ?? '调度策略已生成');
    } catch {
      setToolbarMessage('调度策略生成失败');
    } finally {
      setActionState('idle');
    }
  };

  const handleCreateTask = async () => {
    setActionState('creating');
    setToolbarMessage('');

    try {
      const created = await onCreateTask?.(selectedEdgeId, {
        deviceId: createForm.deviceId.trim(),
        enabled: createForm.enabled,
        intervalMs: Math.max(Number.parseInt(createForm.intervalMs, 10) || 1000, 100),
        pointIds: splitCsv(createForm.pointIds),
        taskId: createForm.taskId.trim(),
      });
      setToolbarMessage(
        created ? `已创建任务 ${created.taskId}` : '已创建任务',
      );
      setCreateDialogOpen(false);
    } catch {
      setToolbarMessage('创建任务失败');
    } finally {
      setActionState('idle');
    }
  };

  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>采集任务编排</h2>
          <p>
            将点位组织成边端可执行的采集任务，统一配置周期、超时、重试、死区和缓存策略。
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
                disabled={actionState === 'scheduling'}
                onClick={() => {
                  void handleGenerateSchedule();
                }}
                type="button"
              >
                <TimerReset size={15} aria-hidden="true" />
                {actionState === 'scheduling' ? '生成中' : '统一调度策略'}
              </button>
              <button
                className="primary-button"
                disabled={actionState === 'creating'}
                onClick={() => setCreateDialogOpen(true)}
                type="button"
              >
                <Plus size={15} aria-hidden="true" />
                新建任务
              </button>
            </>
          ) : null}
        </div>
      </section>

      {createDialogOpen ? (
        <div className="modal-backdrop">
          <form
            aria-labelledby="task-create-dialog-title"
            className="modal-panel"
            onSubmit={(event) => {
              event.preventDefault();
              void handleCreateTask();
            }}
            role="dialog"
          >
            <div className="modal-header">
              <h3 id="task-create-dialog-title">新建采集任务</h3>
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
                <span>Task ID</span>
                <input
                  aria-label="新建 Task ID"
                  required
                  value={createForm.taskId}
                  onChange={(event) =>
                    setCreateForm((current) => ({
                      ...current,
                      taskId: event.target.value,
                    }))
                  }
                />
              </label>
              <label>
                <span>设备 ID</span>
                <input
                  aria-label="新建任务设备 ID"
                  required
                  value={createForm.deviceId}
                  onChange={(event) =>
                    setCreateForm((current) => ({
                      ...current,
                      deviceId: event.target.value,
                    }))
                  }
                />
              </label>
              <label>
                <span>采集点位</span>
                <input
                  aria-label="新建任务采集点位"
                  required
                  value={createForm.pointIds}
                  onChange={(event) =>
                    setCreateForm((current) => ({
                      ...current,
                      pointIds: event.target.value,
                    }))
                  }
                />
              </label>
              <label>
                <span>采集周期(ms)</span>
                <input
                  aria-label="新建任务采集周期(ms)"
                  min="100"
                  step="100"
                  type="number"
                  value={createForm.intervalMs}
                  onChange={(event) =>
                    setCreateForm((current) => ({
                      ...current,
                      intervalMs: event.target.value,
                    }))
                  }
                />
              </label>
              <label>
                <span>启用任务</span>
                <input
                  aria-label="新建任务启用"
                  checked={createForm.enabled}
                  type="checkbox"
                  onChange={(event) =>
                    setCreateForm((current) => ({
                      ...current,
                      enabled: event.target.checked,
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
            <h3>任务清单</h3>
            <span>
              {activeEdge.displayName} · {tasks.length} 个执行计划
            </span>
          </div>
          <div className="table-wrap">
            <table className="ops-table">
              <thead>
                <tr>
                  <th>Task ID</th>
                  <th>设备</th>
                  <th>点位</th>
                  <th>周期</th>
                  <th>状态</th>
                </tr>
              </thead>
              <tbody>
                {visibleTasks.map((task) => (
                  <tr key={`${task.edgeId}:${task.taskId}`}>
                    <td>
                      {isConfigureMode ? (
                        <button
                          aria-label={`选择任务 ${task.taskId}`}
                          aria-pressed={task.taskId === selectedTask.taskId}
                          className="point-id-button"
                          onClick={() => setSelectedTaskId(task.taskId)}
                          type="button"
                        >
                          {task.taskId}
                        </button>
                      ) : (
                        task.taskId
                      )}
                    </td>
                    <td>{task.deviceId}</td>
                    <td>{task.pointList}</td>
                    <td>{task.interval}</td>
                    <td>
                      <span className={task.status === '启用' ? 'tag ok' : 'tag warn'}>
                        {task.status}
                      </span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {tasks.length > pageSize ? (
            <PaginationBar
              ariaLabel="采集任务分页"
              currentPage={currentPage}
              onNext={() => setPage((value) => Math.min(totalPages, value + 1))}
              onPrevious={() => setPage((value) => Math.max(1, value - 1))}
              totalPages={totalPages}
            />
          ) : null}
        </section>

        {isConfigureMode ? (
          <Drawer
          subtitle="保存后进入待发布配置，发布后边端 runtime 调度执行"
          title={`编辑任务 ${selectedTask.taskId}`}
          footer={
            <>
              <span className={`editor-status ${saveState}`} role="status">
                {saveStatusText(saveState)}
              </span>
              <button
                className="secondary-button"
                onClick={() => {
                  setForm(taskToEditorForm(selectedTask));
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
            <h4>任务参数</h4>
            <div className="editor-grid">
              <label className="editor-control">
                <span>设备 ID</span>
                <input
                  value={form.deviceId}
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      deviceId: event.target.value,
                    }))
                  }
                />
              </label>
              <label className="editor-control">
                <span>采集周期(ms)</span>
                <input
                  min="100"
                  step="100"
                  type="number"
                  value={form.intervalMs}
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      intervalMs: event.target.value,
                    }))
                  }
                />
              </label>
            </div>
          </section>
          <section className="drawer-section">
            <h4>点位范围</h4>
            <label className="editor-control">
              <span>采集点位</span>
              <input
                value={form.pointIds}
                onChange={(event) =>
                  setForm((current) => ({
                    ...current,
                    pointIds: event.target.value,
                  }))
                }
              />
            </label>
          </section>
          <section className="drawer-section">
            <h4>执行状态</h4>
            <label className="editor-control">
              <span>启用任务</span>
              <input
                aria-label="启用任务"
                checked={form.enabled}
                type="checkbox"
                onChange={(event) =>
                  setForm((current) => ({
                    ...current,
                    enabled: event.target.checked,
                  }))
                }
              />
            </label>
          </section>
          <DrawerSection
            fields={[
              ['边端', selectedEdgeId],
              ['Task ID', selectedTask.taskId],
              ['当前点位', selectedTask.pointList],
              ['当前状态', selectedTask.status],
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
  deviceId: string;
  enabled: boolean;
  intervalMs: string;
  pointIds: string;
}

function taskToEditorForm(task: CollectionTaskResponse): EditorForm {
  return {
    deviceId: task.deviceId,
    enabled: task.enabled,
    intervalMs: String(task.intervalMs || parseIntervalMs(task.interval)),
    pointIds: task.pointIds.length > 0 ? task.pointIds.join(', ') : task.pointList,
  };
}

function formToSaveRequest(form: EditorForm): SaveCollectionTaskRequest {
  return {
    deviceId: form.deviceId.trim(),
    enabled: form.enabled,
    intervalMs: Math.max(Number.parseInt(form.intervalMs, 10) || 1000, 100),
    pointIds: splitCsv(form.pointIds),
  };
}

function splitCsv(value: string) {
  return value
    .split(',')
    .map((item) => item.trim())
    .filter(Boolean);
}

function parseIntervalMs(interval: string): number {
  return Number.parseInt(interval.replace(/[^\d]/g, ''), 10) || 1000;
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
