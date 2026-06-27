import type {
  AgentActionResponse,
  AlgorithmResponse,
  AuditRecordResponse,
  CollectionTaskResponse,
  CreateEdgeNodeRequest,
  EdgeNodeActionResponse,
  DeviceModelResponse,
  EdgeNodeResponse,
  PointMappingResponse,
  ProtocolConnectionResponse,
  ReleaseListResponse,
  RuntimeStatusResponse,
  ManagementActionResponse,
  SaveAlgorithmRequest,
  SaveCollectionTaskRequest,
  SavePointMappingRequest,
  SaveProtocolConnectionRequest,
  CreateProtocolConnectionRequest,
  SummaryResponse,
} from './types';

export async function fetchSummary(
  fetcher: typeof fetch = fetch,
): Promise<SummaryResponse> {
  return requestJson<SummaryResponse>('/api/summary', fetcher);
}

export async function fetchPointMappings(
  fetcher: typeof fetch = fetch,
): Promise<PointMappingResponse[]> {
  return requestJson<PointMappingResponse[]>('/api/point-mappings', fetcher);
}

export async function fetchEdgePointMappings(
  edgeId: string,
  fetcher: typeof fetch = fetch,
): Promise<PointMappingResponse[]> {
  return requestJson<PointMappingResponse[]>(
    `/api/edges/${encodeURIComponent(edgeId)}/point-mappings`,
    fetcher,
  );
}

export async function fetchReleaseList(
  fetcher: typeof fetch = fetch,
): Promise<ReleaseListResponse> {
  return requestJson<ReleaseListResponse>('/api/releases', fetcher);
}

export async function fetchEdgeNodes(
  fetcher: typeof fetch = fetch,
): Promise<EdgeNodeResponse[]> {
  return requestJson<EdgeNodeResponse[]>('/api/edge-nodes', fetcher);
}

export async function createEdgeNode(
  request: CreateEdgeNodeRequest,
  fetcher: typeof fetch = fetch,
): Promise<EdgeNodeResponse> {
  return requestJson<EdgeNodeResponse>('/api/edge-nodes', fetcher, {
    body: JSON.stringify(request),
    headers: { 'content-type': 'application/json' },
    method: 'POST',
  });
}

export async function rotateEdgeCredentials(
  edgeId: string,
  fetcher: typeof fetch = fetch,
): Promise<EdgeNodeActionResponse> {
  return requestJson<EdgeNodeActionResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/credentials/rotate`,
    fetcher,
    {
      method: 'POST',
    },
  );
}

export async function enableEdgeMaintenanceMode(
  edgeId: string,
  fetcher: typeof fetch = fetch,
): Promise<EdgeNodeActionResponse> {
  return requestJson<EdgeNodeActionResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/maintenance-mode`,
    fetcher,
    {
      method: 'POST',
    },
  );
}

export async function fetchDeviceModels(
  fetcher: typeof fetch = fetch,
): Promise<DeviceModelResponse[]> {
  return requestJson<DeviceModelResponse[]>('/api/device-models', fetcher);
}

export async function createDeviceModelDraft(
  fetcher: typeof fetch = fetch,
): Promise<DeviceModelResponse> {
  return requestJson<DeviceModelResponse>('/api/device-models', fetcher, {
    method: 'POST',
  });
}

export async function fetchProtocolConnections(
  fetcher: typeof fetch = fetch,
): Promise<ProtocolConnectionResponse[]> {
  return requestJson<ProtocolConnectionResponse[]>(
    '/api/protocol-connections',
    fetcher,
  );
}

export async function fetchEdgeProtocolConnections(
  edgeId: string,
  fetcher: typeof fetch = fetch,
): Promise<ProtocolConnectionResponse[]> {
  return requestJson<ProtocolConnectionResponse[]>(
    `/api/edges/${encodeURIComponent(edgeId)}/protocol-connections`,
    fetcher,
  );
}

export async function createPointMappingDraft(
  edgeId: string,
  fetcher: typeof fetch = fetch,
): Promise<PointMappingResponse> {
  return requestJson<PointMappingResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/point-mappings`,
    fetcher,
    {
      method: 'POST',
    },
  );
}

export async function fetchCollectionTasks(
  fetcher: typeof fetch = fetch,
): Promise<CollectionTaskResponse[]> {
  return requestJson<CollectionTaskResponse[]>('/api/collection-tasks', fetcher);
}

export async function fetchEdgeCollectionTasks(
  edgeId: string,
  fetcher: typeof fetch = fetch,
): Promise<CollectionTaskResponse[]> {
  return requestJson<CollectionTaskResponse[]>(
    `/api/edges/${encodeURIComponent(edgeId)}/collection-tasks`,
    fetcher,
  );
}

export async function createCollectionTaskDraft(
  edgeId: string,
  fetcher: typeof fetch = fetch,
): Promise<CollectionTaskResponse> {
  return requestJson<CollectionTaskResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/collection-tasks`,
    fetcher,
    {
      method: 'POST',
    },
  );
}

export async function fetchAlgorithms(
  fetcher: typeof fetch = fetch,
): Promise<AlgorithmResponse[]> {
  return requestJson<AlgorithmResponse[]>('/api/algorithms', fetcher);
}

export async function fetchEdgeAlgorithms(
  edgeId: string,
  fetcher: typeof fetch = fetch,
): Promise<AlgorithmResponse[]> {
  return requestJson<AlgorithmResponse[]>(
    `/api/edges/${encodeURIComponent(edgeId)}/algorithms`,
    fetcher,
  );
}

export async function createAlgorithmDraft(
  edgeId: string,
  fetcher: typeof fetch = fetch,
): Promise<AlgorithmResponse> {
  return requestJson<AlgorithmResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/algorithms`,
    fetcher,
    {
      method: 'POST',
    },
  );
}

export async function fetchAuditRecords(
  fetcher: typeof fetch = fetch,
): Promise<AuditRecordResponse[]> {
  return requestJson<AuditRecordResponse[]>('/api/audit-records', fetcher);
}

export async function fetchRuntimeStatus(
  fetcher: typeof fetch = fetch,
): Promise<RuntimeStatusResponse> {
  return requestJson<RuntimeStatusResponse>('/api/runtime-status', fetcher);
}

export async function savePointMapping(
  pointId: string,
  request: SavePointMappingRequest,
  fetcher: typeof fetch = fetch,
): Promise<PointMappingResponse> {
  return requestJson<PointMappingResponse>(
    `/api/point-mappings/${encodeURIComponent(pointId)}`,
    fetcher,
    {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'PUT',
    },
  );
}

export async function saveEdgePointMapping(
  edgeId: string,
  pointId: string,
  request: SavePointMappingRequest,
  fetcher: typeof fetch = fetch,
): Promise<PointMappingResponse> {
  return requestJson<PointMappingResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/point-mappings/${encodeURIComponent(pointId)}`,
    fetcher,
    {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'PUT',
    },
  );
}

export async function saveEdgeCollectionTask(
  edgeId: string,
  taskId: string,
  request: SaveCollectionTaskRequest,
  fetcher: typeof fetch = fetch,
): Promise<CollectionTaskResponse> {
  return requestJson<CollectionTaskResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/collection-tasks/${encodeURIComponent(taskId)}`,
    fetcher,
    {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'PUT',
    },
  );
}

export async function saveEdgeProtocolConnection(
  edgeId: string,
  connectionId: string,
  request: SaveProtocolConnectionRequest,
  fetcher: typeof fetch = fetch,
): Promise<ProtocolConnectionResponse> {
  return requestJson<ProtocolConnectionResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/protocol-connections/${encodeURIComponent(connectionId)}`,
    fetcher,
    {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'PUT',
    },
  );
}

export async function createEdgeProtocolConnection(
  edgeId: string,
  request: CreateProtocolConnectionRequest,
  fetcher: typeof fetch = fetch,
): Promise<ProtocolConnectionResponse> {
  return requestJson<ProtocolConnectionResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/protocol-connections`,
    fetcher,
    {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'POST',
    },
  );
}

export async function saveEdgeAlgorithm(
  edgeId: string,
  algorithmId: string,
  request: SaveAlgorithmRequest,
  fetcher: typeof fetch = fetch,
): Promise<AlgorithmResponse> {
  return requestJson<AlgorithmResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/algorithms/${encodeURIComponent(algorithmId)}`,
    fetcher,
    {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'PUT',
    },
  );
}

export async function publishLatestRelease(
  edgeId = 'edge-dev',
  fetcher: typeof fetch = fetch,
): Promise<ReleaseListResponse> {
  return requestJson<ReleaseListResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/releases/publish`,
    fetcher,
    {
      method: 'POST',
    },
  );
}

export async function runConfigValidation(
  edgeId: string,
  fetcher: typeof fetch = fetch,
): Promise<ManagementActionResponse> {
  return requestJson<ManagementActionResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/config/validate`,
    fetcher,
    {
      method: 'POST',
    },
  );
}

export async function runReleaseDiff(
  edgeId: string,
  fetcher: typeof fetch = fetch,
): Promise<ManagementActionResponse> {
  return requestJson<ManagementActionResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/releases/diff`,
    fetcher,
    {
      method: 'POST',
    },
  );
}

export async function runAgentSafetyCheck(
  fetcher: typeof fetch = fetch,
): Promise<AgentActionResponse> {
  return requestJson<AgentActionResponse>('/api/agent/safety-check', fetcher, {
    method: 'POST',
  });
}

export async function generateAgentSuggestions(
  fetcher: typeof fetch = fetch,
): Promise<AgentActionResponse> {
  return requestJson<AgentActionResponse>('/api/agent/suggestions', fetcher, {
    method: 'POST',
  });
}

async function requestJson<T>(
  path: string,
  fetcher: typeof fetch,
  init?: RequestInit,
): Promise<T> {
  const response = init === undefined ? await fetcher(path) : await fetcher(path, init);
  if (!response.ok) {
    throw new Error(`Failed to load ${path}: ${response.status}`);
  }

  return response.json() as Promise<T>;
}
