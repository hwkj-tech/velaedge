import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { Modal } from './Modal';

describe('Modal', () => {
  it('closes when Escape is pressed', () => {
    const onClose = vi.fn();

    render(
      <Modal onClose={onClose}>
        <section aria-label="测试弹窗" role="dialog">
          测试内容
        </section>
      </Modal>,
    );

    fireEvent.keyDown(document, { key: 'Escape' });

    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('only closes the topmost modal when multiple modals are open', () => {
    const onBottomClose = vi.fn();
    const onTopClose = vi.fn();

    render(
      <>
        <Modal onClose={onBottomClose}>
          <section aria-label="底层弹窗" role="dialog">
            底层
          </section>
        </Modal>
        <Modal onClose={onTopClose}>
          <section aria-label="顶层弹窗" role="dialog">
            顶层
          </section>
        </Modal>
      </>,
    );

    expect(screen.getByRole('dialog', { name: '顶层弹窗' })).toBeInTheDocument();

    fireEvent.keyDown(document, { key: 'Escape' });

    expect(onTopClose).toHaveBeenCalledTimes(1);
    expect(onBottomClose).not.toHaveBeenCalled();
  });
});
