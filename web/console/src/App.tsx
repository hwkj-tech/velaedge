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
  deleteEdgeAlgorithm,
  deleteEdgeCollectionTask,
  deleteEdgeDataConfig,
  deleteDeviceModel,
  deleteEdgeNode,
  deleteEdgePointMapping,
  deleteEdgeProtocolConnection,
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
import { AlgorithmsPage } from './pages/AlgorithmsPage';
import { AuditLogPage } from './pages/AuditLogPage';
import { DashboardPage } from './pages/DashboardPage';
import { DataConfigsPage } from './pages/DataConfigsPage';
import { DeviceModelsPage } from './pages/DeviceModelsPage';
import { DiscoveryPage } from './pages/DiscoveryPage';
import {
  EdgeNodesPage,
  type EdgeConfigSummary,
  type EdgeConfigTabKey,
} from './pages/EdgeNodesPage';
import { CollectionTasksPage } from './pages/CollectionTasksPage';
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

const configurationPages = new Set<PageKey>(['edgeConfig']);
const deprecatedGlobalConfigurationPages = new Set<PageKey>([
  'protocolConnections',
  'dataConfigs',
  'releases',
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
  const [edgeConfigInitialTab, setEdgeConfigInitialTab] =
    useState<EdgeConfigTabKey>('overview');
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

  const handleDeletePoint = async (edgeId: string, pointId: string) => {
    await deleteEdgePointMapping(edgeId, pointId);
    const [nextPointMappings, nextReleaseList] = await Promise.all([
      fetchEdgePointMappings(edgeId),
      fetchReleaseList(),
    ]);
    setPointMappings(nextPointMappings);
    setReleaseList(nextReleaseList);
    setSelectedPointEdgeId(edgeId);
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
        details: ['没有可导入的候选点位'],
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

  const handleDeleteCollectionTask = async (edgeId: string, taskId: string) => {
    await deleteEdgeCollectionTask(edgeId, taskId);
    const [nextCollectionTasks, nextReleaseList] = await Promise.all([
      fetchEdgeCollectionTasks(edgeId),
      fetchReleaseList(),
    ]);
    setCollectionTasks(nextCollectionTasks);
    setReleaseList(nextReleaseList);
    setSelectedCollectionEdgeId(edgeId);
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

  const handleDeleteProtocolConnection = async (
    edgeId: string,
    connectionId: string,
  ) => {
    await deleteEdgeProtocolConnection(edgeId, connectionId);
    const [nextProtocolConnections, nextReleaseList] = await Promise.all([
      fetchEdgeProtocolConnections(edgeId),
      fetchReleaseList(),
    ]);
    setProtocolConnections(nextProtocolConnections);
    setReleaseList(nextReleaseList);
    setSelectedProtocolEdgeId(edgeId);
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

  const handleDeleteAlgorithm = async (edgeId: string, algorithmId: string) => {
    await deleteEdgeAlgorithm(edgeId, algorithmId);
    const [nextAlgorithms, nextReleaseList] = await Promise.all([
      fetchEdgeAlgorithms(edgeId),
      fetchReleaseList(),
    ]);
    setAlgorithms(nextAlgorithms);
    setReleaseList(nextReleaseList);
    setSelectedAlgorithmEdgeId(edgeId);
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

  const handleDeleteDeviceModel = async (deviceType: string) => {
    await deleteDeviceModel(deviceType);
    setDeviceModels(await fetchDeviceModels());
    setReleaseList(await fetchReleaseList());
  };

  const handleDeleteEdgeNode = async (edgeId: string) => {
    await deleteEdgeNode(edgeId);
    const [nextEdges, nextSummary, nextReleaseList] = await Promise.all([
      fetchEdgeNodes(),
      fetchSummary(),
      fetchReleaseList(),
    ]);
    setEdgeNodes(nextEdges);
    setSummary(nextSummary);
    setReleaseList(nextReleaseList);
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

  const handleConfigureEdge = async (
    edgeId: string,
    tab: EdgeConfigTabKey = 'overview',
  ) => {
    setEdgeConfigurationMode('configure');
    setEdgeConfigInitialTab(tab);
    setFocusedRuntimeEdgeId(undefined);
    setSelectedProtocolEdgeId(edgeId);
    setSelectedPointEdgeId(edgeId);
    setSelectedCollectionEdgeId(edgeId);
    setSelectedDataConfigEdgeId(edgeId);
    setSelectedAlgorithmEdgeId(edgeId);
    setActivePage('edgeConfig');

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
    if (deprecatedGlobalConfigurationPages.has(activePage)) {
      setEdgeConfigurationMode('list');
      setActivePage('edges');
    }
  }, [activePage]);

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
        edgeConfigInitialTab,
        handleAgentSafetyCheck,
        handleAssessAlgorithmRisk,
        handleConfigureEdge,
        handleDeleteEdgeNode,
        handleCreateAlgorithm,
        handleSaveAlgorithm,
        handleDeleteAlgorithm,
        handleCreateCollectionTask,
        handleDeleteCollectionTask,
        handleCreateDeviceModel,
        handleSaveDeviceModel,
        handleDeleteDeviceModel,
        handleCreatePoint,
        handleDeletePoint,
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
        handleDeleteProtocolConnection,
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

function buildEdgeConfigSummaries(
  edges: EdgeNodeResponse[] = [],
  protocolConnections: ProtocolConnectionResponse[] = [],
  pointMappings: PointMappingResponse[] = [],
  collectionTasks: CollectionTaskResponse[] = [],
  dataConfigs: DataConfigResponse[] = [],
  mqttUplink?: MqttUplinkResponse,
  releaseList?: ReleaseListResponse,
): EdgeConfigSummary[] {
  return edges.map((edge) => {
    const releaseResult = releaseList?.applyResults.find(
      (result) => result.edgeId === edge.edgeId,
    );
    return {
      collectionTaskCount: collectionTasks.filter((task) => task.edgeId === edge.edgeId)
        .length,
      dataConfigCount: dataConfigs.filter((config) => config.edgeId === edge.edgeId)
        .length,
      edgeId: edge.edgeId,
      mqttSinkId: mqttUplink?.sinkId ?? '未配置',
      pointCount: pointMappings.filter((point) => point.edgeId === edge.edgeId).length,
      protocolCount: protocolConnections.filter(
        (connection) => connection.edgeId === edge.edgeId,
      ).length,
      releaseStatus: formatReleaseBindingStatus(releaseResult),
    };
  });
}

function renderPage(
  activePage: PageKey,
  summary: SummaryResponse,
  loadState: 'loading' | 'ready' | 'error',
  edgeConfigurationMode: EdgeConfigurationMode,
  focusedRuntimeEdgeId: string | undefined,
  edgeConfigInitialTab: EdgeConfigTabKey,
  onAgentSafetyCheck: () => Promise<AgentActionResponse>,
  onAssessAlgorithmRisk: (edgeId: string) => Promise<ManagementActionResponse>,
  onConfigureEdge: (edgeId: string, tab?: EdgeConfigTabKey) => Promise<void>,
  onDeleteEdge: (edgeId: string) => Promise<void>,
  onCreateAlgorithm: (
    edgeId: string,
    request: CreateAlgorithmRequest,
  ) => Promise<AlgorithmResponse>,
  onSaveAlgorithm: (
    edgeId: string,
    algorithmId: string,
    request: SaveAlgorithmRequest,
  ) => Promise<void>,
  onDeleteAlgorithm: (edgeId: string, algorithmId: string) => Promise<void>,
  onCreateCollectionTask: (
    edgeId: string,
    request: CreateCollectionTaskRequest,
  ) => Promise<CollectionTaskResponse>,
  onDeleteCollectionTask: (edgeId: string, taskId: string) => Promise<void>,
  onCreateDeviceModel: (request: CreateDeviceModelRequest) => Promise<DeviceModelResponse>,
  onSaveDeviceModel: (
    deviceType: string,
    request: SaveDeviceModelRequest,
  ) => Promise<DeviceModelResponse>,
  onDeleteDeviceModel: (deviceType: string) => Promise<void>,
  onCreatePoint: (
    edgeId?: string,
    request?: CreatePointMappingRequest,
  ) => Promise<PointMappingResponse>,
  onDeletePoint: (edgeId: string, pointId: string) => Promise<void>,
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
  onDeleteProtocolConnection: (
    edgeId: string,
    connectionId: string,
  ) => Promise<void>,
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
          configSummaries={buildEdgeConfigSummaries(
            edgeNodes,
            protocolConnections,
            pointMappings,
            collectionTasks,
            dataConfigs,
            mqttUplink,
            releaseList,
          )}
          edges={edgeNodes}
          mqttUplink={mqttUplink}
          onConfigureEdge={(edgeId, tab) => {
            void onConfigureEdge(edgeId, tab);
          }}
          onDeleteEdge={onDeleteEdge}
          onMonitorEdge={onMonitorEdge}
          onSaveMqttUplink={onSaveMqttUplink}
        />
      );
    case 'edgeConfig':
      return (
        <EdgeConfigWorkspace
          algorithms={algorithms}
          collectionTasks={collectionTasks}
          dataConfigs={dataConfigs}
          discoverySuggestions={discoverySuggestions}
          edgeId={selectedProtocolEdgeId}
          edges={edgeNodes}
          initialTab={edgeConfigInitialTab}
          mqttUplink={mqttUplink}
          onAssessAlgorithmRisk={onAssessAlgorithmRisk}
          onCreateAlgorithm={onCreateAlgorithm}
          onCreateCollectionTask={onCreateCollectionTask}
          onCreatePoint={onCreatePoint}
          onCreateProtocolConnection={onCreateProtocolConnection}
          onDeleteAlgorithm={onDeleteAlgorithm}
          onDeleteCollectionTask={onDeleteCollectionTask}
          onDeleteDataConfig={onDeleteDataConfig}
          onDeletePoint={onDeletePoint}
          onDeleteProtocolConnection={onDeleteProtocolConnection}
          onGenerateSchedule={onGenerateSchedule}
          onImportPoints={onImportPoints}
          onPublish={onPublish}
          onReleaseDiff={onReleaseDiff}
          onRunDiscovery={onRunDiscovery}
          onSaveAlgorithm={onSaveAlgorithm}
          onSaveCollectionTask={onSaveCollectionTask}
          onSaveDataConfig={onSaveDataConfig}
          onSaveMqttUplink={onSaveMqttUplink}
          onSavePoint={onSavePoint}
          onSaveProtocolConnection={onSaveProtocolConnection}
          onValidateConfig={onValidateConfig}
          pointMappings={pointMappings}
          protocolConnections={protocolConnections}
          releaseList={releaseList}
        />
      );
    case 'deviceModels':
      return (
        <DeviceModelsPage
          deviceModels={deviceModels}
          onCreateDeviceModel={onCreateDeviceModel}
          onDeleteDeviceModel={onDeleteDeviceModel}
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
          onDeleteConnection={onDeleteProtocolConnection}
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

type EdgeConfigTab =
  | 'overview'
  | 'protocol'
  | 'points'
  | 'collection'
  | 'algorithms'
  | 'reports'
  | 'mqtt'
  | 'discovery'
  | 'release';

function EdgeConfigWorkspace({
  algorithms,
  collectionTasks,
  dataConfigs,
  discoverySuggestions,
  edgeId,
  edges,
  initialTab,
  mqttUplink,
  onCreateCollectionTask,
  onCreateAlgorithm,
  onCreatePoint,
  onCreateProtocolConnection,
  onAssessAlgorithmRisk,
  onDeleteAlgorithm,
  onDeleteCollectionTask,
  onDeleteDataConfig,
  onDeletePoint,
  onDeleteProtocolConnection,
  onGenerateSchedule,
  onImportPoints,
  onPublish,
  onReleaseDiff,
  onRunDiscovery,
  onSaveCollectionTask,
  onSaveAlgorithm,
  onSaveDataConfig,
  onSaveMqttUplink,
  onSavePoint,
  onSaveProtocolConnection,
  onValidateConfig,
  pointMappings,
  protocolConnections,
  releaseList,
}: {
  algorithms?: AlgorithmResponse[];
  collectionTasks?: CollectionTaskResponse[];
  dataConfigs?: DataConfigResponse[];
  discoverySuggestions?: PointMappingSuggestionResponse[];
  edgeId: string;
  edges?: EdgeNodeResponse[];
  initialTab: EdgeConfigTabKey;
  mqttUplink?: MqttUplinkResponse;
  onCreateCollectionTask: (
    edgeId: string,
    request: CreateCollectionTaskRequest,
  ) => Promise<CollectionTaskResponse>;
  onCreateAlgorithm: (
    edgeId: string,
    request: CreateAlgorithmRequest,
  ) => Promise<AlgorithmResponse>;
  onCreatePoint: (
    edgeId?: string,
    request?: CreatePointMappingRequest,
  ) => Promise<PointMappingResponse>;
  onCreateProtocolConnection: (
    edgeId: string,
    request: CreateProtocolConnectionRequest,
  ) => Promise<ProtocolConnectionResponse>;
  onAssessAlgorithmRisk: (edgeId: string) => Promise<ManagementActionResponse>;
  onDeleteAlgorithm: (edgeId: string, algorithmId: string) => Promise<void>;
  onDeleteCollectionTask: (edgeId: string, taskId: string) => Promise<void>;
  onDeleteDataConfig: (edgeId: string, configId: string) => Promise<void>;
  onDeletePoint: (edgeId: string, pointId: string) => Promise<void>;
  onDeleteProtocolConnection: (
    edgeId: string,
    connectionId: string,
  ) => Promise<void>;
  onGenerateSchedule: (edgeId: string) => Promise<ManagementActionResponse>;
  onImportPoints: (edgeId: string) => Promise<ManagementActionResponse>;
  onPublish: (edgeId: string) => Promise<void>;
  onReleaseDiff: (edgeId: string) => Promise<ManagementActionResponse>;
  onRunDiscovery: (
    edgeId: string,
    request: RunDiscoveryRequest,
  ) => Promise<DiscoveryReportResponse>;
  onSaveCollectionTask: (
    edgeId: string,
    taskId: string,
    request: SaveCollectionTaskRequest,
  ) => Promise<void>;
  onSaveAlgorithm: (
    edgeId: string,
    algorithmId: string,
    request: SaveAlgorithmRequest,
  ) => Promise<void>;
  onSaveDataConfig: (
    edgeId: string,
    configId: string | null,
    request: SaveDataConfigRequest,
  ) => Promise<void>;
  onSaveMqttUplink: (
    edgeId: string,
    request: MqttUplinkResponse,
  ) => Promise<MqttUplinkResponse>;
  onSavePoint: (
    edgeId: string,
    pointId: string,
    request: SavePointMappingRequest,
  ) => Promise<void>;
  onSaveProtocolConnection: (
    edgeId: string,
    connectionId: string,
    request: SaveProtocolConnectionRequest,
  ) => Promise<void>;
  onValidateConfig: (edgeId?: string) => Promise<ManagementActionResponse>;
  pointMappings?: PointMappingResponse[];
  protocolConnections?: ProtocolConnectionResponse[];
  releaseList?: ReleaseListResponse;
}) {
  const [activeTab, setActiveTab] = useState<EdgeConfigTab>(initialTab);
  const edge = edges?.find((item) => item.edgeId === edgeId);
  const tabs: Array<{ key: EdgeConfigTab; label: string }> = [
    { key: 'overview', label: '配置总览' },
    { key: 'protocol', label: '协议连接' },
    { key: 'points', label: '点位配置' },
    { key: 'collection', label: '采集任务' },
    { key: 'algorithms', label: '算法配置' },
    { key: 'reports', label: '数据上报' },
    { key: 'mqtt', label: 'MQTT' },
    { key: 'discovery', label: '点位探测' },
    { key: 'release', label: '配置发布' },
  ];

  useEffect(() => {
    setActiveTab(initialTab);
  }, [edgeId, initialTab]);

  return (
    <div className="edge-config-workspace">
      <section className="edge-config-header">
        <div>
          <span>边端配置工作区</span>
          <h2>{edge?.displayName ?? edgeId}</h2>
          <p>{edge?.site ?? '未分组'} · {edgeId} · {edge?.runtimeId ?? 'runtime 未上报'}</p>
        </div>
        <div className="edge-config-status">
          <span className={edge?.status === '健康' ? 'tag ok' : 'tag warn'}>
            {edge?.status ?? '未知'}
          </span>
          <strong>{edge?.resources ?? '-'}</strong>
          <small>{edge?.heartbeat ?? '-'}</small>
        </div>
      </section>

      <nav className="workspace-tabs" aria-label="边端配置标签" role="tablist">
        {tabs.map((tab) => (
          <button
            aria-selected={activeTab === tab.key}
            className={activeTab === tab.key ? 'workspace-tab active' : 'workspace-tab'}
            key={tab.key}
            onClick={() => setActiveTab(tab.key)}
            role="tab"
            type="button"
          >
            {tab.label}
          </button>
        ))}
      </nav>

      <section className="workspace-tab-panel" role="tabpanel">
        {activeTab === 'overview' ? (
          <EdgeConfigOverview
            collectionTasks={collectionTasks}
            dataConfigs={dataConfigs}
            edgeId={edgeId}
            mqttUplink={mqttUplink}
            onPublish={onPublish}
            onValidateConfig={onValidateConfig}
            pointMappings={pointMappings}
            protocolConnections={protocolConnections}
            releaseList={releaseList}
            setActiveTab={setActiveTab}
          />
        ) : null}
        {activeTab === 'protocol' ? (
          <ProtocolConnectionsPage
            connections={protocolConnections}
            edges={edges}
            embedded
            mode="configure"
            onCreateConnection={onCreateProtocolConnection}
            onDeleteConnection={onDeleteProtocolConnection}
            onSaveConnection={onSaveProtocolConnection}
            onValidateConnection={onValidateConfig}
            selectedEdgeId={edgeId}
          />
        ) : null}
        {activeTab === 'points' ? (
          <PointMappingsPage
            edges={edges}
            embedded
            mode="configure"
            onCreatePoint={onCreatePoint}
            onDeletePoint={onDeletePoint}
            onImportPoints={onImportPoints}
            onSavePoint={onSavePoint}
            onValidateDraft={onValidateConfig}
            points={pointMappings}
            selectedEdgeId={edgeId}
          />
        ) : null}
        {activeTab === 'collection' ? (
          <CollectionTasksPage
            edges={edges}
            embedded
            mode="configure"
            onCreateTask={onCreateCollectionTask}
            onDeleteTask={onDeleteCollectionTask}
            onGenerateSchedule={onGenerateSchedule}
            onSaveTask={onSaveCollectionTask}
            selectedEdgeId={edgeId}
            tasks={collectionTasks}
          />
        ) : null}
        {activeTab === 'algorithms' ? (
          <AlgorithmsPage
            algorithms={algorithms}
            edges={edges}
            embedded
            mode="configure"
            onAssessRisk={onAssessAlgorithmRisk}
            onCreateAlgorithm={onCreateAlgorithm}
            onDeleteAlgorithm={onDeleteAlgorithm}
            onSaveAlgorithm={onSaveAlgorithm}
            selectedEdgeId={edgeId}
          />
        ) : null}
        {activeTab === 'reports' ? (
          <DataConfigsPage
            algorithms={algorithms}
            configs={dataConfigs}
            edges={edges}
            embedded
            mqttUplink={mqttUplink}
            onDeleteConfig={onDeleteDataConfig}
            onSaveConfig={onSaveDataConfig}
            pointMappings={pointMappings}
            protocolConnections={protocolConnections}
            selectedEdgeId={edgeId}
          />
        ) : null}
        {activeTab === 'mqtt' ? (
          <MqttUplinkPage
            onSave={onSaveMqttUplink}
            selectedEdgeId={edgeId}
            uplink={mqttUplink}
          />
        ) : null}
        {activeTab === 'discovery' ? (
          <DiscoveryPage
            onRunDiscovery={onRunDiscovery}
            selectedEdgeId={edgeId}
            suggestions={discoverySuggestions}
          />
        ) : null}
        {activeTab === 'release' ? (
          <ReleasesPage
            edges={edges}
            onPublish={onPublish}
            onShowDiff={onReleaseDiff}
            onValidateRelease={onValidateConfig}
            releaseList={releaseList}
            selectedEdgeId={edgeId}
          />
        ) : null}
      </section>
    </div>
  );
}

function EdgeConfigOverview({
  collectionTasks = [],
  dataConfigs = [],
  edgeId,
  mqttUplink,
  onPublish,
  onValidateConfig,
  pointMappings = [],
  protocolConnections = [],
  releaseList,
  setActiveTab,
}: {
  collectionTasks?: CollectionTaskResponse[];
  dataConfigs?: DataConfigResponse[];
  edgeId: string;
  mqttUplink?: MqttUplinkResponse;
  onPublish: (edgeId: string) => Promise<void>;
  onValidateConfig: (edgeId?: string) => Promise<ManagementActionResponse>;
  pointMappings?: PointMappingResponse[];
  protocolConnections?: ProtocolConnectionResponse[];
  releaseList?: ReleaseListResponse;
  setActiveTab: (tab: EdgeConfigTab) => void;
}) {
  const [actionMessage, setActionMessage] = useState('');
  const [actionState, setActionState] = useState<'idle' | 'validating' | 'publishing'>('idle');
  const releaseResult = releaseList?.applyResults.find((result) => result.edgeId === edgeId);
  const readiness = calculateOverviewReadiness({
    collectionTasks,
    dataConfigs,
    mqttUplink,
    pointMappings,
    protocolConnections,
    releaseResult,
  });
  const summaryCards = [
    { label: '协议连接', value: protocolConnections.length, tab: 'protocol' as const },
    { label: '点位配置', value: pointMappings.length, tab: 'points' as const },
    { label: '采集任务', value: collectionTasks.length, tab: 'collection' as const },
    { label: '数据上报', value: dataConfigs.length, tab: 'reports' as const },
  ];
  const bindingRows: Array<{
    action: string;
    description: string;
    label: string;
    status: string;
    tab: EdgeConfigTab;
  }> = [
    {
      action: '配置连接',
      description: '串口总线、Modbus RTU/TCP、DL/T645 等采集通道',
      label: '协议连接',
      status: `${protocolConnections.length} 个连接`,
      tab: 'protocol',
    },
    {
      action: '配置点位',
      description: '协议地址到语义点位的映射，发布后由 runtime 执行采集',
      label: '点位配置',
      status: `${pointMappings.length} 个点位`,
      tab: 'points',
    },
    {
      action: '配置任务',
      description: '采集周期、点位批次、超时重试和缓存策略',
      label: '采集任务',
      status: `${collectionTasks.length} 个任务`,
      tab: 'collection',
    },
    {
      action: '配置上报',
      description: '点位组合、DSL 算法、JSON 结构和 MQTT topic',
      label: '数据上报',
      status: `${dataConfigs.length} 套配置`,
      tab: 'reports',
    },
    {
      action: '配置 MQTT',
      description: 'velaMQ broker、clientId、QoS、批量和刷新策略',
      label: 'MQTT 上报',
      status: mqttUplink ? mqttUplink.sinkId : '未配置',
      tab: 'mqtt',
    },
    {
      action: '发布配置',
      description: '校验配置差异，将选中的边端配置包发布到 runtime',
      label: '配置发布',
      status: formatReleaseBindingStatus(releaseResult),
      tab: 'release',
    },
  ];

  const handleValidate = async () => {
    setActionState('validating');
    setActionMessage('');
    try {
      const result = await onValidateConfig(edgeId);
      setActionMessage(result.message || `校验${result.status ? ` ${result.status}` : '完成'}`);
    } catch {
      setActionMessage('配置校验失败');
    } finally {
      setActionState('idle');
    }
  };

  const handlePublish = async () => {
    setActionState('publishing');
    setActionMessage('');
    try {
      await onPublish(edgeId);
      setActionMessage('已发布到 runtime 待应用');
    } catch {
      setActionMessage('发布失败');
    } finally {
      setActionState('idle');
    }
  };

  return (
    <div className="edge-config-overview">
      <section className="page-intro">
        <div>
          <h2>配置绑定总览</h2>
          <p>
            这里展示该边端已选择的采集、处理、上报和发布配置。新增配置后，从这里进入对应配置项继续配置并发布到 runtime。
          </p>
        </div>
      </section>

      <section className="overview-command-strip" aria-label="边端配置发布状态">
        <div className="overview-readiness">
          <span>配置完整度</span>
          <strong>{readiness}%</strong>
          <div className="readiness-track">
            <span style={{ width: `${readiness}%` }} />
          </div>
          <small>{readiness >= 84 ? '可发布到 runtime' : '建议先补齐连接、点位、上报和 MQTT'}</small>
        </div>
        <div className="overview-release-state">
          <span>当前发布状态</span>
          <strong>{formatReleaseBindingStatus(releaseResult)}</strong>
          {actionMessage ? <small role="status">{actionMessage}</small> : null}
        </div>
        <div className="overview-actions">
          <button
            className="secondary-button"
            disabled={actionState !== 'idle'}
            onClick={() => {
              void handleValidate();
            }}
            type="button"
          >
            {actionState === 'validating' ? '校验中' : '校验配置'}
          </button>
          <button
            className="primary-button"
            disabled={actionState !== 'idle'}
            onClick={() => {
              void handlePublish();
            }}
            type="button"
          >
            {actionState === 'publishing' ? '发布中' : '发布到 runtime'}
          </button>
        </div>
      </section>

      <div className="binding-summary-grid" aria-label="边端配置绑定统计">
        {summaryCards.map((card) => (
          <button
            className="binding-summary-card"
            key={card.label}
            onClick={() => setActiveTab(card.tab)}
            type="button"
          >
            <span>{card.label}</span>
            <strong>{card.value}</strong>
            <small className={card.value > 0 ? 'summary-card-state ready' : 'summary-card-state pending'}>
              {card.value > 0 ? '已配置' : '待配置'}
            </small>
          </button>
        ))}
      </div>

      <section className="panel">
        <div className="panel-header">
          <h3>配置绑定清单</h3>
          <span>选择后配置该边端能力</span>
        </div>
        <div className="binding-matrix">
          {bindingRows.map((row) => (
            <div className="binding-row" key={row.label}>
              <div>
                <strong>{row.label}</strong>
                <p>{row.description}</p>
              </div>
              <span className="binding-status">{row.status}</span>
              <button
                className="secondary-button compact"
                onClick={() => setActiveTab(row.tab)}
                type="button"
              >
                {row.action}
              </button>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}

function calculateOverviewReadiness({
  collectionTasks,
  dataConfigs,
  mqttUplink,
  pointMappings,
  protocolConnections,
  releaseResult,
}: {
  collectionTasks: CollectionTaskResponse[];
  dataConfigs: DataConfigResponse[];
  mqttUplink?: MqttUplinkResponse;
  pointMappings: PointMappingResponse[];
  protocolConnections: ProtocolConnectionResponse[];
  releaseResult: ReleaseListResponse['applyResults'][number] | undefined;
}) {
  const completed = [
    protocolConnections.length > 0,
    pointMappings.length > 0,
    collectionTasks.length > 0,
    dataConfigs.length > 0,
    Boolean(mqttUplink?.sinkId),
    Boolean(releaseResult && !releaseResult.result.includes('待')),
  ].filter(Boolean).length;
  return Math.round((completed / 6) * 100);
}

function formatReleaseBindingStatus(
  releaseResult: ReleaseListResponse['applyResults'][number] | undefined,
) {
  if (!releaseResult) {
    return '待发布';
  }
  const version =
    releaseResult.reportedVersion && releaseResult.reportedVersion !== '-'
      ? releaseResult.reportedVersion
      : releaseResult.desiredVersion;
  return version && version !== '-' ? `${releaseResult.result} · ${version}` : releaseResult.result;
}
