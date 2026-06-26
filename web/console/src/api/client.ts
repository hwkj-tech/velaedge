import type {
  PointMappingResponse,
  ReleaseListResponse,
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
