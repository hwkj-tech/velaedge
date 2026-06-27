import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { ProtocolConnectionsPage } from './ProtocolConnectionsPage';

describe('ProtocolConnectionsPage', () => {
  it('shows protocol connection table and editor fields', () => {
    render(<ProtocolConnectionsPage selectedEdgeId="edge-dev" />);

    expect(screen.getByText('连接清单')).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: '选择连接 modbus-line-a' }),
    ).toBeInTheDocument();
    expect(screen.getByText('编辑连接 modbus-line-a')).toBeInTheDocument();
    expect(screen.getByLabelText('协议类型')).toBeInTheDocument();
  });

  it('saves edited protocol connection drafts from the editor drawer', async () => {
    const onSaveConnection = vi.fn().mockResolvedValue(undefined);

    render(
      <ProtocolConnectionsPage
        selectedEdgeId="edge-dev"
        onSaveConnection={onSaveConnection}
      />,
    );

    fireEvent.change(screen.getByLabelText('协议类型'), {
      target: { value: 'OpcUa' },
    });
    fireEvent.change(screen.getByLabelText('端点'), {
      target: { value: 'opc.tcp://10.12.0.80:4840' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存草稿' }));

    await waitFor(() => {
      expect(onSaveConnection).toHaveBeenCalledWith(
        'edge-dev',
        'modbus-line-a',
        {
          endpoint: 'opc.tcp://10.12.0.80:4840',
          protocolType: 'OpcUa',
        },
      );
    });
    expect(screen.getByText('草稿已保存')).toBeInTheDocument();
  });

  it('switches the active edge before editing protocol connections', async () => {
    const onSelectEdge = vi.fn().mockResolvedValue(undefined);

    render(
      <ProtocolConnectionsPage
        edges={[
          {
            edgeId: 'edge-dev',
            displayName: '研发实验室边端',
            site: '研发/实验室',
            runtimeId: 'runtime-dev',
            status: '健康',
            resources: '18% / 42% / 61%',
            heartbeat: '8 秒前',
            capabilities: ['protocol:modbus-tcp'],
          },
          {
            edgeId: 'edge-prod',
            displayName: '产线边端',
            site: '制造/一线',
            runtimeId: 'runtime-prod',
            status: '健康',
            resources: '22% / 48% / 66%',
            heartbeat: '6 秒前',
            capabilities: ['protocol:opcua'],
          },
        ]}
        selectedEdgeId="edge-dev"
        onSelectEdge={onSelectEdge}
      />,
    );

    fireEvent.change(screen.getByLabelText('配置边端'), {
      target: { value: 'edge-prod' },
    });

    await waitFor(() => {
      expect(onSelectEdge).toHaveBeenCalledWith('edge-prod');
    });
  });
});
