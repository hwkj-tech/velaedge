import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { AuthGate } from './AuthGate';

describe('AuthGate', () => {
  it('submits a non-empty access token', async () => {
    const onAuthenticate = vi.fn().mockResolvedValue(undefined);
    render(<AuthGate onAuthenticate={onAuthenticate} />);

    fireEvent.change(screen.getByLabelText('访问令牌'), {
      target: { value: 'operator-token' },
    });
    fireEvent.click(screen.getByRole('button', { name: '进入控制台' }));

    await waitFor(() => expect(onAuthenticate).toHaveBeenCalledWith('operator-token'));
  });

  it('keeps the login surface open and reports rejected credentials', async () => {
    render(
      <AuthGate
        onAuthenticate={vi.fn().mockRejectedValue(new Error('访问令牌无效或已失效'))}
      />,
    );

    fireEvent.change(screen.getByLabelText('访问令牌'), {
      target: { value: 'expired-token' },
    });
    fireEvent.submit(screen.getByLabelText('访问令牌').closest('form')!);

    expect(await screen.findByRole('alert')).toHaveTextContent('访问令牌无效或已失效');
  });
});
