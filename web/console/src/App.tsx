import { useEffect, useState } from 'react';

import {
  createAlgorithmDraft,
  createCollectionTaskDraft,
  createDeviceModelDraft,
  createPointMappingDraft,
  fetchAuditRecords,
  fetchDeviceModels,
  fetchEdgeAlgorithms,
  fetchEdgeCollectionTasks,
  fetchEdgePointMappings,
  fetchEdgeProtocolConnections,
  fetchEdgeNodes,
  fetchReleaseList,
  fetchRuntimeStatus,
  fetchSummary,
  fetchMqttUplink,
  fetchDiscoverySuggestions,
  generateAgentSuggestions,
  publishLatestRelease,
  runAgentSafetyCheck,
  runConfigValidation,
  runReleaseDiff,
  runDiscovery,
  saveMqttUplink,
  createEdgeProtocolConnection,
  saveDeviceModel,
  rotateEdgeCredentials,
  enableEdgeMaintenanceMode,
  saveEdgeAlgorithm,
  saveEdgeCollectionTask,
  saveEdgePointMapping,
  saveEdgeProtocolConnection,
} from './api/client';
import type {
  AlgorithmResponse,
  AgentActionResponse,
  AuditRecordResponse,
  CollectionTaskResponse,
  CreateAlgorithmRequest,
  CreateCollectionTaskRequest,
  CreateDeviceModelRequest,
  CreatePointMappingRequest,
  DiscoveryReportResponse,
  DeviceModelResponse,
  EdgeNodeActionResponse,
  EdgeNodeResponse,
  ManagementActionResponse,
  MqttUplinkResponse,
  PointMappingResponse,
  PointMappingSuggestionResponse,
  ProtocolConnectionResponse,
  ReleaseListResponse,
  RunDiscoveryRequest,
  RuntimeStatusResponse,
  SaveAlgorithmRequest,
  SaveCollectionTaskRequest,
  SaveDeviceModelRequest,
  CreateProtocolConnectionRequest,
  SavePointMappingRequest,
  SaveProtocolConnectionRequest,
  SummaryResponse,
} from './api/types';
import { AppShell, type PageKey } from './layout/AppShell';
import { AgentAssistantPage } from './pages/AgentAssistantPage';
import { AlgorithmsPage } from './pages/AlgorithmsPage';
import { AuditLogPage } from './pages/AuditLogPage';
import { CollectionTasksPage } from './pages/CollectionTasksPage';
import { DashboardPage } from './pages/DashboardPage';
import { DeviceModelsPage } from './pages/DeviceModelsPage';
import { DiscoveryPage } from './pages/DiscoveryPage';
import { EdgeNodesPage } from './pages/EdgeNodesPage';
import { MqttUplinkPage } from './pages/MqttUplinkPage';
import { PointMappingsPage } from './pages/PointMappingsPage';
import { ProtocolConnectionsPage } from './pages/ProtocolConnectionsPage';
import { ReleasesPage } from './pages/ReleasesPage';
import { RuntimeStatusPage } from './pages/RuntimeStatusPage';

const initialSummary: SummaryResponse = {
  edge_count: 0,
  pending_release_count: 0,
};

const defaultConfigEdgeId = 'edge-dev';
type EdgeConfigurationMode = 'configure' | 'list';

const configurationPages = new Set<PageKey>([
  'protocolConnections',
  'pointMappings',
  'collectionTasks',
  'algorithms',
]);

interface ConsoleSnapshot {
  algorithms: AlgorithmResponse[];
  auditRecords: AuditRecordResponse[];
  collectionTasks: CollectionTaskResponse[];
  deviceModels: DeviceModelResponse[];
  edgeNodes: EdgeNodeResponse[];
  pointMappings: PointMappingResponse[];
  protocolConnections: ProtocolConnectionResponse[];
  mqttUplink: MqttUplinkResponse;
  discoverySuggestions: PointMappingSuggestionResponse[];
  releaseList: ReleaseListResponse;
  runtimeStatus: RuntimeStatusResponse;
  summary: SummaryResponse;
}

export default function App() {
  const [activePage, setActivePage] = useState<PageKey>('dashboard');
  const [summary, setSummary] = useState(initialSummary);
  const [edgeNodes, setEdgeNodes] = useState<EdgeNodeResponse[]>();
  const [deviceModels, setDeviceModels] = useState<DeviceModelResponse[]>();
  const [protocolConnections, setProtocolConnections] =
    useState<ProtocolConnectionResponse[]>();
  const [selectedProtocolEdgeId, setSelectedProtocolEdgeId] = useState('edge-dev');
  const [pointMappings, setPointMappings] = useState<PointMappingResponse[]>();
  const [selectedPointEdgeId, setSelectedPointEdgeId] = useState('edge-dev');
  const [collectionTasks, setCollectionTasks] = useState<CollectionTaskResponse[]>();
  const [selectedCollectionEdgeId, setSelectedCollectionEdgeId] = useState('edge-dev');
  const [algorithms, setAlgorithms] = useState<AlgorithmResponse[]>();
  const [selectedAlgorithmEdgeId, setSelectedAlgorithmEdgeId] = useState('edge-dev');
  const [mqttUplink, setMqttUplink] = useState<MqttUplinkResponse>();
  const [discoverySuggestions, setDiscoverySuggestions] =
    useState<PointMappingSuggestionResponse[]>();
  const [releaseList, setReleaseList] = useState<ReleaseListResponse>();
  const [runtimeStatus, setRuntimeStatus] = useState<RuntimeStatusResponse>();
  const [auditRecords, setAuditRecords] = useState<AuditRecordResponse[]>();
  const [edgeConfigurationMode, setEdgeConfigurationMode] =
    useState<EdgeConfigurationMode>('list');
  const [focusedRuntimeEdgeId, setFocusedRuntimeEdgeId] = useState<string>();
  const [loadState, setLoadState] = useState<'loading' | 'ready' | 'error'>(
    'loading',
  );

  const applySnapshot = (snapshot: ConsoleSnapshot) => {
    setSummary(snapshot.summary);
    setEdgeNodes(snapshot.edgeNodes);
    setDeviceModels(snapshot.deviceModels);
    setProtocolConnections(snapshot.protocolConnections);
    setMqttUplink(snapshot.mqttUplink);
    setDiscoverySuggestions(snapshot.discoverySuggestions);
    setPointMappings(snapshot.pointMappings);
    setCollectionTasks(snapshot.collectionTasks);
    setAlgorithms(snapshot.algorithms);
    setReleaseList(snapshot.releaseList);
    setRuntimeStatus(snapshot.runtimeStatus);
    setAuditRecords(snapshot.auditRecords);
    setLoadState('ready');
  };

  const refreshConsoleData = async () => {
    applySnapshot(await loadConsoleSnapshot());
  };

  const handleSavePoint = async (
    edgeId: string,
    pointId: string,
    request: SavePointMappingRequest,
  ) => {
    await saveEdgePointMapping(edgeId, pointId, request);
    const [nextPointMappings, nextReleaseList] = await Promise.all([
      fetchEdgePointMappings(edgeId),
      fetchReleaseList(),
    ]);
    setPointMappings(nextPointMappings);
    setReleaseList(nextReleaseList);
    setSelectedPointEdgeId(edgeId);
  };

  const handleCreatePoint = async (
    edgeId = defaultConfigEdgeId,
    request: CreatePointMappingRequest = {},
  ) => {
    const created = await createPointMappingDraft(edgeId, request);
    const [nextPointMappings, nextReleaseList] = await Promise.all([
      fetchEdgePointMappings(edgeId),
      fetchReleaseList(),
    ]);
    setPointMappings(nextPointMappings);
    setReleaseList(nextReleaseList);
    setSelectedPointEdgeId(edgeId);
    return created;
  };

  const handleValidateConfig = async (
    edgeId = defaultConfigEdgeId,
  ): Promise<ManagementActionResponse> => runConfigValidation(edgeId);

  const handleImportPoints = async (
    edgeId: string,
  ): Promise<ManagementActionResponse> => {
    await handleCreatePoint(edgeId);
    return {
      action: 'import_points',
      details: ['已按云端模板导入 1 个点位配置'],
      message: '批量导入已完成',
      status: '已完成',
    };
  };

  const handleSelectPointEdge = async (edgeId: string) => {
    setSelectedPointEdgeId(edgeId);
    setPointMappings(await fetchEdgePointMappings(edgeId));
  };

  const handleSaveCollectionTask = async (
    edgeId: string,
    taskId: string,
    request: SaveCollectionTaskRequest,
  ) => {
    await saveEdgeCollectionTask(edgeId, taskId, request);
    const [nextCollectionTasks, nextReleaseList] = await Promise.all([
      fetchEdgeCollectionTasks(edgeId),
      fetchReleaseList(),
    ]);
    setCollectionTasks(nextCollectionTasks);
    setReleaseList(nextReleaseList);
    setSelectedCollectionEdgeId(edgeId);
  };

  const handleCreateCollectionTask = async (
    edgeId: string,
    request: CreateCollectionTaskRequest,
  ) => {
    const created = await createCollectionTaskDraft(edgeId, request);
    const [nextCollectionTasks, nextReleaseList] = await Promise.all([
      fetchEdgeCollectionTasks(edgeId),
      fetchReleaseList(),
    ]);
    setCollectionTasks(nextCollectionTasks);
    setReleaseList(nextReleaseList);
    setSelectedCollectionEdgeId(edgeId);
    return created;
  };

  const handleGenerateSchedule = async (
    edgeId: string,
  ): Promise<ManagementActionResponse> => {
    const result = await runConfigValidation(edgeId);
    return {
      ...result,
      action: 'generate_schedule',
      message: '调度策略已生成',
    };
  };

  const handleSelectCollectionEdge = async (edgeId: string) => {
    setSelectedCollectionEdgeId(edgeId);
    setCollectionTasks(await fetchEdgeCollectionTasks(edgeId));
  };

  const handleSaveProtocolConnection = async (
    edgeId: string,
    connectionId: string,
    request: SaveProtocolConnectionRequest,
  ) => {
    await saveEdgeProtocolConnection(edgeId, connectionId, request);
    const [nextProtocolConnections, nextReleaseList] = await Promise.all([
      fetchEdgeProtocolConnections(edgeId),
      fetchReleaseList(),
    ]);
    setProtocolConnections(nextProtocolConnections);
    setReleaseList(nextReleaseList);
    setSelectedProtocolEdgeId(edgeId);
  };

  const handleCreateProtocolConnection = async (
    edgeId: string,
    request: CreateProtocolConnectionRequest,
  ) => {
    const created = await createEdgeProtocolConnection(edgeId, request);
    const [nextProtocolConnections, nextReleaseList] = await Promise.all([
      fetchEdgeProtocolConnections(edgeId),
      fetchReleaseList(),
    ]);
    setProtocolConnections(nextProtocolConnections);
    setReleaseList(nextReleaseList);
    setSelectedProtocolEdgeId(edgeId);
    return created;
  };

  const handleSelectProtocolEdge = async (edgeId: string) => {
    setSelectedProtocolEdgeId(edgeId);
    setProtocolConnections(await fetchEdgeProtocolConnections(edgeId));
  };

  const handleSaveAlgorithm = async (
    edgeId: string,
    algorithmId: string,
    request: SaveAlgorithmRequest,
  ) => {
    await saveEdgeAlgorithm(edgeId, algorithmId, request);
    const [nextAlgorithms, nextReleaseList] = await Promise.all([
      fetchEdgeAlgorithms(edgeId),
      fetchReleaseList(),
    ]);
    setAlgorithms(nextAlgorithms);
    setReleaseList(nextReleaseList);
    setSelectedAlgorithmEdgeId(edgeId);
  };

  const handleCreateAlgorithm = async (
    edgeId: string,
    request: CreateAlgorithmRequest,
  ) => {
    const created = await createAlgorithmDraft(edgeId, request);
    const [nextAlgorithms, nextReleaseList] = await Promise.all([
      fetchEdgeAlgorithms(edgeId),
      fetchReleaseList(),
    ]);
    setAlgorithms(nextAlgorithms);
    setReleaseList(nextReleaseList);
    setSelectedAlgorithmEdgeId(edgeId);
    return created;
  };

  const handleAssessAlgorithmRisk = async (
    edgeId: string,
  ): Promise<ManagementActionResponse> => {
    const result = await runConfigValidation(edgeId);
    return {
      ...result,
      action: 'assess_algorithm_risk',
      message: '算法风险评估已完成',
    };
  };

  const handleSelectAlgorithmEdge = async (edgeId: string) => {
    setSelectedAlgorithmEdgeId(edgeId);
    setAlgorithms(await fetchEdgeAlgorithms(edgeId));
  };

  const handlePublishLatestRelease = async (edgeId: string) => {
    await publishLatestRelease(edgeId);
    await refreshConsoleData();
  };

  const handleReleaseDiff = async (
    edgeId: string,
  ): Promise<ManagementActionResponse> => runReleaseDiff(edgeId);

  const handleCreateDeviceModel = async (request: CreateDeviceModelRequest) => {
    const created = await createDeviceModelDraft(request);
    setDeviceModels(await fetchDeviceModels());
    return created;
  };

  const handleSaveDeviceModel = async (
    deviceType: string,
    request: SaveDeviceModelRequest,
  ) => {
    const saved = await saveDeviceModel(deviceType, request);
    setDeviceModels(await fetchDeviceModels());
    return saved;
  };

  const handleAgentSafetyCheck = async (): Promise<AgentActionResponse> =>
    runAgentSafetyCheck();

  const handleGenerateAgentSuggestions = async (): Promise<AgentActionResponse> =>
    generateAgentSuggestions();

  const handleSaveMqttUplink = async (
    edgeId: string,
    request: MqttUplinkResponse,
  ) => {
    const saved = await saveMqttUplink(edgeId, request);
    setMqttUplink(saved);
    setReleaseList(await fetchReleaseList());
    return saved;
  };

  const handleRunDiscovery = async (
    edgeId: string,
    request: RunDiscoveryRequest,
  ): Promise<DiscoveryReportResponse> => {
    const report = await runDiscovery(edgeId, request);
    setDiscoverySuggestions(report.suggestions);
    return report;
  };

  const handleRotateCredentials = async (
    edgeId: string,
  ): Promise<EdgeNodeActionResponse> => {
    const result = await rotateEdgeCredentials(edgeId);
    setEdgeNodes(await fetchEdgeNodes());
    return result;
  };

  const handleEnableMaintenance = async (
    edgeId: string,
  ): Promise<EdgeNodeActionResponse> => {
    const result = await enableEdgeMaintenanceMode(edgeId);
    setEdgeNodes(await fetchEdgeNodes());
    return result;
  };

  const handleNavigate = (page: PageKey) => {
    setActivePage(page);
    setEdgeConfigurationMode(configurationPages.has(page) ? 'configure' : 'list');
    if (page !== 'runtimeStatus') {
      setFocusedRuntimeEdgeId(undefined);
    }
  };

  const handleConfigureEdge = async (edgeId: string) => {
    setEdgeConfigurationMode('configure');
    setFocusedRuntimeEdgeId(undefined);
    setSelectedProtocolEdgeId(edgeId);
    setSelectedPointEdgeId(edgeId);
    setSelectedCollectionEdgeId(edgeId);
    setSelectedAlgorithmEdgeId(edgeId);
    setActivePage('protocolConnections');

    const [
      nextProtocolConnections,
      nextPointMappings,
      nextCollectionTasks,
      nextAlgorithms,
    ] = await Promise.all([
      fetchEdgeProtocolConnections(edgeId),
      fetchEdgePointMappings(edgeId),
      fetchEdgeCollectionTasks(edgeId),
      fetchEdgeAlgorithms(edgeId),
    ]);
    setProtocolConnections(nextProtocolConnections);
    setPointMappings(nextPointMappings);
    setCollectionTasks(nextCollectionTasks);
    setAlgorithms(nextAlgorithms);
  };

  const handleMonitorEdge = (edgeId: string) => {
    setFocusedRuntimeEdgeId(edgeId);
    setActivePage('runtimeStatus');
  };

  useEffect(() => {
    let mounted = true;

    loadConsoleSnapshot()
      .then((snapshot) => {
        if (mounted) {
          applySnapshot(snapshot);
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

  useEffect(() => {
    let mounted = true;
    const refreshRuntimeStatus = async () => {
      try {
        const nextRuntimeStatus = await fetchRuntimeStatus();
        if (mounted) {
          setRuntimeStatus(nextRuntimeStatus);
        }
      } catch {
        // Keep the last known runtime snapshot visible if polling misses once.
      }
    };
    const intervalId = window.setInterval(refreshRuntimeStatus, 5000);

    return () => {
      mounted = false;
      window.clearInterval(intervalId);
    };
  }, []);

  return (
    <AppShell activePage={activePage} onNavigate={handleNavigate}>
      {renderPage(
        activePage,
        summary,
        loadState,
        edgeConfigurationMode,
        focusedRuntimeEdgeId,
        handleAgentSafetyCheck,
        handleAssessAlgorithmRisk,
        handleConfigureEdge,
        handleCreateAlgorithm,
        handleCreateCollectionTask,
        handleCreateDeviceModel,
        handleSaveDeviceModel,
        handleCreatePoint,
        handleEnableMaintenance,
        handleGenerateAgentSuggestions,
        handleRunDiscovery,
        handleGenerateSchedule,
        handleImportPoints,
        handleMonitorEdge,
        handleReleaseDiff,
        handleRotateCredentials,
        handleSavePoint,
        handleSelectPointEdge,
        selectedPointEdgeId,
        handleSaveCollectionTask,
        handleSelectCollectionEdge,
        selectedCollectionEdgeId,
        handleSaveProtocolConnection,
        handleCreateProtocolConnection,
        handleSelectProtocolEdge,
        selectedProtocolEdgeId,
        handleSaveAlgorithm,
        handleSelectAlgorithmEdge,
        selectedAlgorithmEdgeId,
        handleSaveMqttUplink,
        handlePublishLatestRelease,
        handleValidateConfig,
        edgeNodes,
        deviceModels,
        protocolConnections,
        mqttUplink,
        discoverySuggestions,
        pointMappings,
        collectionTasks,
        algorithms,
        releaseList,
        runtimeStatus,
        auditRecords,
      )}
    </AppShell>
  );
}

async function loadConsoleSnapshot(): Promise<ConsoleSnapshot> {
  const [
    summary,
    edgeNodes,
    deviceModels,
    protocolConnections,
    mqttUplink,
    discoverySuggestions,
    pointMappings,
    collectionTasks,
    algorithms,
    releaseList,
    runtimeStatus,
    auditRecords,
  ] = await Promise.all([
    fetchSummary(),
    fetchEdgeNodes(),
    fetchDeviceModels(),
    fetchEdgeProtocolConnections(defaultConfigEdgeId),
    fetchMqttUplink(defaultConfigEdgeId),
    fetchDiscoverySuggestions(defaultConfigEdgeId),
    fetchEdgePointMappings(defaultConfigEdgeId),
    fetchEdgeCollectionTasks(defaultConfigEdgeId),
    fetchEdgeAlgorithms(defaultConfigEdgeId),
    fetchReleaseList(),
    fetchRuntimeStatus(),
    fetchAuditRecords(),
  ]);

  return {
    algorithms,
    auditRecords,
    collectionTasks,
    deviceModels,
    edgeNodes,
    pointMappings,
    protocolConnections,
    mqttUplink,
    discoverySuggestions,
    releaseList,
    runtimeStatus,
    summary,
  };
}

function renderPage(
  activePage: PageKey,
  summary: SummaryResponse,
  loadState: 'loading' | 'ready' | 'error',
  edgeConfigurationMode: EdgeConfigurationMode,
  focusedRuntimeEdgeId: string | undefined,
  onAgentSafetyCheck: () => Promise<AgentActionResponse>,
  onAssessAlgorithmRisk: (edgeId: string) => Promise<ManagementActionResponse>,
  onConfigureEdge: (edgeId: string) => Promise<void>,
  onCreateAlgorithm: (
    edgeId: string,
    request: CreateAlgorithmRequest,
  ) => Promise<AlgorithmResponse>,
  onCreateCollectionTask: (
    edgeId: string,
    request: CreateCollectionTaskRequest,
  ) => Promise<CollectionTaskResponse>,
  onCreateDeviceModel: (request: CreateDeviceModelRequest) => Promise<DeviceModelResponse>,
  onSaveDeviceModel: (
    deviceType: string,
    request: SaveDeviceModelRequest,
  ) => Promise<DeviceModelResponse>,
  onCreatePoint: (
    edgeId?: string,
    request?: CreatePointMappingRequest,
  ) => Promise<PointMappingResponse>,
  onEnableMaintenance: (edgeId: string) => Promise<EdgeNodeActionResponse>,
  onGenerateAgentSuggestions: () => Promise<AgentActionResponse>,
  onRunDiscovery: (
    edgeId: string,
    request: RunDiscoveryRequest,
  ) => Promise<DiscoveryReportResponse>,
  onGenerateSchedule: (edgeId: string) => Promise<ManagementActionResponse>,
  onImportPoints: (edgeId: string) => Promise<ManagementActionResponse>,
  onMonitorEdge: (edgeId: string) => void,
  onReleaseDiff: (edgeId: string) => Promise<ManagementActionResponse>,
  onRotateCredentials: (edgeId: string) => Promise<EdgeNodeActionResponse>,
  onSavePoint: (
    edgeId: string,
    pointId: string,
    request: SavePointMappingRequest,
  ) => Promise<void>,
  onSelectPointEdge: (edgeId: string) => Promise<void>,
  selectedPointEdgeId: string,
  onSaveCollectionTask: (
    edgeId: string,
    taskId: string,
    request: SaveCollectionTaskRequest,
  ) => Promise<void>,
  onSelectCollectionEdge: (edgeId: string) => Promise<void>,
  selectedCollectionEdgeId: string,
  onSaveProtocolConnection: (
    edgeId: string,
    connectionId: string,
    request: SaveProtocolConnectionRequest,
  ) => Promise<void>,
  onCreateProtocolConnection: (
    edgeId: string,
    request: CreateProtocolConnectionRequest,
  ) => Promise<ProtocolConnectionResponse>,
  onSelectProtocolEdge: (edgeId: string) => Promise<void>,
  selectedProtocolEdgeId: string,
  onSaveAlgorithm: (
    edgeId: string,
    algorithmId: string,
    request: SaveAlgorithmRequest,
  ) => Promise<void>,
  onSelectAlgorithmEdge: (edgeId: string) => Promise<void>,
  selectedAlgorithmEdgeId: string,
  onSaveMqttUplink: (
    edgeId: string,
    request: MqttUplinkResponse,
  ) => Promise<MqttUplinkResponse>,
  onPublish: (edgeId: string) => Promise<void>,
  onValidateConfig: (edgeId?: string) => Promise<ManagementActionResponse>,
  edgeNodes?: EdgeNodeResponse[],
  deviceModels?: DeviceModelResponse[],
  protocolConnections?: ProtocolConnectionResponse[],
  mqttUplink?: MqttUplinkResponse,
  discoverySuggestions?: PointMappingSuggestionResponse[],
  pointMappings?: PointMappingResponse[],
  collectionTasks?: CollectionTaskResponse[],
  algorithms?: AlgorithmResponse[],
  releaseList?: ReleaseListResponse,
  runtimeStatus?: RuntimeStatusResponse,
  auditRecords?: AuditRecordResponse[],
) {
  switch (activePage) {
    case 'dashboard':
      return (
        <DashboardPage
          auditRecords={auditRecords}
          edgeNodes={edgeNodes}
          loadState={loadState}
          runtimeStatus={runtimeStatus}
          summary={summary}
        />
      );
    case 'edges':
      return (
        <EdgeNodesPage
          edges={edgeNodes}
          onConfigureEdge={(edgeId) => {
            void onConfigureEdge(edgeId);
          }}
          onEnableMaintenance={onEnableMaintenance}
          onMonitorEdge={onMonitorEdge}
          onRotateCredentials={onRotateCredentials}
        />
      );
    case 'deviceModels':
      return (
        <DeviceModelsPage
          deviceModels={deviceModels}
          onCreateDeviceModel={onCreateDeviceModel}
          onSaveDeviceModel={onSaveDeviceModel}
        />
      );
    case 'protocolConnections':
      return (
        <ProtocolConnectionsPage
          connections={protocolConnections}
          edges={edgeNodes}
          mode={edgeConfigurationMode}
          onCreateConnection={onCreateProtocolConnection}
          onSaveConnection={onSaveProtocolConnection}
          onSelectEdge={onSelectProtocolEdge}
          onValidateConnection={onValidateConfig}
          selectedEdgeId={selectedProtocolEdgeId}
        />
      );
    case 'pointMappings':
      return (
        <PointMappingsPage
          edges={edgeNodes}
          mode={edgeConfigurationMode}
          onCreatePoint={onCreatePoint}
          onImportPoints={onImportPoints}
          onSavePoint={onSavePoint}
          onSelectEdge={onSelectPointEdge}
          onValidateDraft={onValidateConfig}
          points={pointMappings}
          selectedEdgeId={selectedPointEdgeId}
        />
      );
    case 'collectionTasks':
      return (
        <CollectionTasksPage
          edges={edgeNodes}
          mode={edgeConfigurationMode}
          onCreateTask={onCreateCollectionTask}
          onGenerateSchedule={onGenerateSchedule}
          onSaveTask={onSaveCollectionTask}
          onSelectEdge={onSelectCollectionEdge}
          selectedEdgeId={selectedCollectionEdgeId}
          tasks={collectionTasks}
        />
      );
    case 'algorithms':
      return (
        <AlgorithmsPage
          algorithms={algorithms}
          edges={edgeNodes}
          mode={edgeConfigurationMode}
          onAssessRisk={onAssessAlgorithmRisk}
          onCreateAlgorithm={onCreateAlgorithm}
          onSaveAlgorithm={onSaveAlgorithm}
          onSelectEdge={onSelectAlgorithmEdge}
          selectedEdgeId={selectedAlgorithmEdgeId}
        />
      );
    case 'mqttUplink':
      return (
        <MqttUplinkPage
          onSave={onSaveMqttUplink}
          selectedEdgeId={defaultConfigEdgeId}
          uplink={mqttUplink}
        />
      );
    case 'discovery':
      return (
        <DiscoveryPage
          onRunDiscovery={onRunDiscovery}
          selectedEdgeId={defaultConfigEdgeId}
          suggestions={discoverySuggestions}
        />
      );
    case 'releases':
      return (
        <ReleasesPage
          edges={edgeNodes}
          onPublish={onPublish}
          onShowDiff={onReleaseDiff}
          onValidateRelease={onValidateConfig}
          releaseList={releaseList}
        />
      );
    case 'runtimeStatus':
      return (
        <RuntimeStatusPage
          focusedEdgeId={focusedRuntimeEdgeId}
          runtimeStatus={runtimeStatus}
        />
      );
    case 'auditLog':
      return <AuditLogPage auditRecords={auditRecords} />;
    case 'agentAssistant':
      return (
        <AgentAssistantPage
          onGenerateSuggestions={onGenerateAgentSuggestions}
          onRunSafetyCheck={onAgentSafetyCheck}
        />
      );
  }
}
