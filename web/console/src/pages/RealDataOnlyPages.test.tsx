import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { AgentAssistantPage } from './AgentAssistantPage';
import { AlgorithmsPage } from './AlgorithmsPage';
import { AuditLogPage } from './AuditLogPage';
import { CollectionTasksPage } from './CollectionTasksPage';
import { DeviceModelsPage } from './DeviceModelsPage';
import { EdgeNodesPage } from './EdgeNodesPage';
import { PointMappingsPage } from './PointMappingsPage';
import { ProtocolConnectionsPage } from './ProtocolConnectionsPage';

describe('management pages use API data only', () => {
  it('renders explicit empty states without synthesizing demo records', () => {
    const views = [
      <AlgorithmsPage key="algorithms" />,
      <CollectionTasksPage key="tasks" />,
      <DeviceModelsPage key="models" />,
      <EdgeNodesPage key="edges" />,
      <PointMappingsPage key="points" />,
      <ProtocolConnectionsPage key="connections" />,
      <AuditLogPage key="audit" />,
      <AgentAssistantPage key="agent" />,
    ];

    for (const view of views) {
      const { unmount } = render(view);
      expect(screen.queryByText('edge-dev')).not.toBeInTheDocument();
      expect(screen.queryByText('modbus-line-a')).not.toBeInTheDocument();
      expect(screen.queryByText('pressure-change-report')).not.toBeInTheDocument();
      expect(screen.queryByText('pump-main')).not.toBeInTheDocument();
      unmount();
    }
  });
});
