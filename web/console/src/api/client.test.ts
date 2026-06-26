import { describe, expect, it, vi } from 'vitest';

import { fetchSummary } from './client';

describe('fetchSummary', () => {
  it('loads cloud summary from the API', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ edge_count: 2, pending_release_count: 1 }),
    });

    const result = await fetchSummary(fetchMock as unknown as typeof fetch);

    expect(result.edge_count).toBe(2);
    expect(result.pending_release_count).toBe(1);
  });
});
