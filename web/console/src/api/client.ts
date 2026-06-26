import type {
  AlgorithmResponse,
  AuditRecordResponse,
  CollectionTaskResponse,
  DeviceModelResponse,
  EdgeNodeResponse,
  PointMappingResponse,
  ProtocolConnectionResponse,
  ReleaseListResponse,
  RuntimeStatusResponse,
  SavePointMappingRequest,
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

export async function fetchDeviceModels(
  fetcher: typeof fetch = fetch,
): Promise<DeviceModelResponse[]> {
  return requestJson<DeviceModelResponse[]>('/api/device-models', fetcher);
}

export async function fetchProtocolConnections(
  fetcher: typeof fetch = fetch,
): Promise<ProtocolConnectionResponse[]> {
  return requestJson<ProtocolConnectionResponse[]>(
    '/api/protocol-connections',
    fetcher,
  );
}

export async function fetchCollectionTasks(
  fetcher: typeof fetch = fetch,
): Promise<CollectionTaskResponse[]> {
  return requestJson<CollectionTaskResponse[]>('/api/collection-tasks', fetcher);
}

export async function fetchAlgorithms(
  fetcher: typeof fetch = fetch,
): Promise<AlgorithmResponse[]> {
  return requestJson<AlgorithmResponse[]>('/api/algorithms', fetcher);
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

export async function publishLatestRelease(
  fetcher: typeof fetch = fetch,
): Promise<ReleaseListResponse> {
  return requestJson<ReleaseListResponse>('/api/releases/publish', fetcher, {
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
