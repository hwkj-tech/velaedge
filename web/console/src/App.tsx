import { useEffect, useState } from 'react';

import {
  createAlgorithmDraft,
  createCollectionTaskDraft,
  createEdgeDataConfig,
  createDeviceModelDraft,
  createPointMappingDraft,
  fetchAuditRecords,
  fetchDeviceModels,
  fetchEdgeAlgorithms,
  fetchEdgeCollectionTasks,
  fetchEdgeDataConfigs,
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
  deleteEdgeDataConfig,
  createEdgeProtocolConnection,
  saveDeviceModel,
  saveEdgeAlgorithm,
  saveEdgeCollectionTask,
  saveEdgeDataConfig,
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
  DataConfigResponse,
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
  SaveDataConfigRequest,
  SaveDeviceModelRequest,
  CreateProtocolConnectionRequest,
  SavePointMappingRequest,
  SaveProtocolConnectionRequest,
  SummaryResponse,
} from './api/types';
import { AppShell, type PageKey } from './layout/AppShell';
import { AgentAssistantPage } from './pages/AgentAssistantPage';
import { AuditLogPage } from './pages/AuditLogPage';
import { DashboardPage } from './pages/DashboardPage';
import { DataConfigsPage } from './pages/DataConfigsPage';
import { DeviceModelsPage } from './pages/DeviceModelsPage';
import { DiscoveryPage } from './pages/DiscoveryPage';
import { EdgeNodesPage } from './pages/EdgeNodesPage';
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
  'dataConfigs',
]);

interface ConsoleSnapshot {
  algorithms: AlgorithmResponse[];
  auditRecords: AuditRecordResponse[];
  collectionTasks: CollectionTaskResponse[];
  dataConfigs: DataConfigResponse[];
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
  const [dataConfigs, setDataConfigs] = useState<DataConfigResponse[]>();
  const [selectedDataConfigEdgeId, setSelectedDataConfigEdgeId] = useState('edge-dev');
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
    setDataConfigs(snapshot.dataConfigs);
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
    const [suggestions, currentPoints, currentConnections] = await Promise.all([
      fetchDiscoverySuggestions(edgeId),
      fetchEdgePointMappings(edgeId),
      fetchEdgeProtocolConnections(edgeId),
    ]);
    const existingPointIds = new Set(currentPoints.map((point) => point.pointId));
    const knownDeviceIds = new Set(currentPoints.map((point) => point.deviceId));
    const knownConnectionIds = new Set(
      currentConnections.map((connection) => connection.connectionId),
    );
    const importableSuggestions = uniqueSuggestions(suggestions).filter(
      (suggestion) =>
        !existingPointIds.has(suggestion.pointId) &&
        knownConnectionIds.has(suggestion.protocolConnectionId) &&
        (knownDeviceIds.size === 0 || knownDeviceIds.has(suggestion.deviceId)),
    );

    if (importableSuggestions.length === 0) {
      return {
        action: 'import_points',
        details: ['当前边端没有可导入的候选点位'],
        message: '没有可导入的候选点位',
        status: '未变更',
      };
    }

    for (const suggestion of importableSuggestions) {
      await createPointMappingDraft(edgeId, suggestionToPointRequest(suggestion));
    }

    const [nextPointMappings, nextReleaseList, nextSuggestions] = await Promise.all([
      fetchEdgePointMappings(edgeId),
      fetchReleaseList(),
      fetchDiscoverySuggestions(edgeId),
    ]);
    setPointMappings(nextPointMappings);
    setReleaseList(nextReleaseList);
    setDiscoverySuggestions(nextSuggestions);
    setSelectedPointEdgeId(edgeId);

    return {
      action: 'import_points',
      details: importableSuggestions.map(
        (suggestion) =>
          `${suggestion.pointId} -> ${suggestion.protocolConnectionId}:${suggestion.address}`,
      ),
      message: `已导入 ${importableSuggestions.length} 个候选点位`,
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

  const handleSaveDataConfig = async (
    edgeId: string,
    configId: string | null,
    request: SaveDataConfigRequest,
  ) => {
    if (configId) {
      await saveEdgeDataConfig(edgeId, configId, request);
    } else {
      await createEdgeDataConfig(edgeId, request);
    }
    const [nextDataConfigs, nextReleaseList] = await Promise.all([
      fetchEdgeDataConfigs(edgeId),
      fetchReleaseList(),
    ]);
    setDataConfigs(nextDataConfigs);
    setReleaseList(nextReleaseList);
    setSelectedDataConfigEdgeId(edgeId);
  };

  const handleDeleteDataConfig = async (edgeId: string, configId: string) => {
    await deleteEdgeDataConfig(edgeId, configId);
    const [nextDataConfigs, nextReleaseList] = await Promise.all([
      fetchEdgeDataConfigs(edgeId),
      fetchReleaseList(),
    ]);
    setDataConfigs(nextDataConfigs);
    setReleaseList(nextReleaseList);
    setSelectedDataConfigEdgeId(edgeId);
  };

  const handleSelectDataConfigEdge = async (edgeId: string) => {
    setSelectedDataConfigEdgeId(edgeId);
    setDataConfigs(await fetchEdgeDataConfigs(edgeId));
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
    setSelectedDataConfigEdgeId(edgeId);
    setSelectedAlgorithmEdgeId(edgeId);
    setActivePage('dataConfigs');

    const [
      nextProtocolConnections,
      nextPointMappings,
      nextCollectionTasks,
      nextDataConfigs,
      nextAlgorithms,
    ] = await Promise.all([
      fetchEdgeProtocolConnections(edgeId),
      fetchEdgePointMappings(edgeId),
      fetchEdgeCollectionTasks(edgeId),
      fetchEdgeDataConfigs(edgeId),
      fetchEdgeAlgorithms(edgeId),
    ]);
    setProtocolConnections(nextProtocolConnections);
    setPointMappings(nextPointMappings);
    setCollectionTasks(nextCollectionTasks);
    setDataConfigs(nextDataConfigs);
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
        handleGenerateAgentSuggestions,
        handleRunDiscovery,
        handleGenerateSchedule,
        handleImportPoints,
        handleMonitorEdge,
        handleReleaseDiff,
        handleSavePoint,
        handleSelectPointEdge,
        selectedPointEdgeId,
        handleSaveCollectionTask,
        handleSelectCollectionEdge,
        selectedCollectionEdgeId,
        handleSaveDataConfig,
        handleDeleteDataConfig,
        selectedDataConfigEdgeId,
        handleSaveProtocolConnection,
        handleCreateProtocolConnection,
        selectedProtocolEdgeId,
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
        dataConfigs,
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
    dataConfigs,
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
    fetchEdgeDataConfigs(defaultConfigEdgeId),
    fetchEdgeAlgorithms(defaultConfigEdgeId),
    fetchReleaseList(),
    fetchRuntimeStatus(),
    fetchAuditRecords(),
  ]);

  return {
    algorithms,
    auditRecords,
    collectionTasks,
    dataConfigs,
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

function suggestionToPointRequest(
  suggestion: PointMappingSuggestionResponse,
): CreatePointMappingRequest {
  const [addressKind, ...addressParts] = suggestion.address.split(':');
  return {
    addressKind: addressKind || 'simulated',
    addressValue: addressParts.join(':') || suggestion.pointId,
    connectionId: suggestion.protocolConnectionId,
    deviceId: suggestion.deviceId,
    pointId: suggestion.pointId,
    semanticId: suggestion.semanticId,
    unit: suggestion.unit,
    valueType: suggestion.valueType,
  };
}

function uniqueSuggestions(suggestions: PointMappingSuggestionResponse[]) {
  const seen = new Set<string>();
  return suggestions.filter((suggestion) => {
    if (seen.has(suggestion.pointId)) {
      return false;
    }
    seen.add(suggestion.pointId);
    return true;
  });
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
  onGenerateAgentSuggestions: () => Promise<AgentActionResponse>,
  onRunDiscovery: (
    edgeId: string,
    request: RunDiscoveryRequest,
  ) => Promise<DiscoveryReportResponse>,
  onGenerateSchedule: (edgeId: string) => Promise<ManagementActionResponse>,
  onImportPoints: (edgeId: string) => Promise<ManagementActionResponse>,
  onMonitorEdge: (edgeId: string) => void,
  onReleaseDiff: (edgeId: string) => Promise<ManagementActionResponse>,
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
  onSaveDataConfig: (
    edgeId: string,
    configId: string | null,
    request: SaveDataConfigRequest,
  ) => Promise<void>,
  onDeleteDataConfig: (edgeId: string, configId: string) => Promise<void>,
  selectedDataConfigEdgeId: string,
  onSaveProtocolConnection: (
    edgeId: string,
    connectionId: string,
    request: SaveProtocolConnectionRequest,
  ) => Promise<void>,
  onCreateProtocolConnection: (
    edgeId: string,
    request: CreateProtocolConnectionRequest,
  ) => Promise<ProtocolConnectionResponse>,
  selectedProtocolEdgeId: string,
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
  dataConfigs?: DataConfigResponse[],
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
          mqttUplink={mqttUplink}
          onConfigureEdge={(edgeId) => {
            void onConfigureEdge(edgeId);
          }}
          onMonitorEdge={onMonitorEdge}
          onSaveMqttUplink={onSaveMqttUplink}
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
          onValidateConnection={onValidateConfig}
          selectedEdgeId={selectedProtocolEdgeId}
        />
      );
    case 'dataConfigs':
      return (
        <DataConfigsPage
          algorithms={algorithms}
          configs={dataConfigs}
          edges={edgeNodes}
          mqttUplink={mqttUplink}
          onDeleteConfig={onDeleteDataConfig}
          onSaveConfig={onSaveDataConfig}
          pointMappings={pointMappings}
          protocolConnections={protocolConnections}
          selectedEdgeId={selectedDataConfigEdgeId}
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
