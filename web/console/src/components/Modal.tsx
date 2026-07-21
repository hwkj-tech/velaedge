import { type ReactNode, useEffect, useRef } from 'react';

export function Modal({
  children,
  onClose,
}: {
  children: ReactNode;
  onClose?: () => void;
}) {
  const backdropRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!onClose) return undefined;

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || event.defaultPrevented || event.isComposing) {
        return;
      }

      const openModals = Array.from(
        document.querySelectorAll('[data-modal-backdrop="true"]'),
      );
      if (openModals[openModals.length - 1] !== backdropRef.current) {
        return;
      }

      event.preventDefault();
      onClose();
    };

    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [onClose]);

  return (
    <div className="modal-backdrop" data-modal-backdrop="true" ref={backdropRef}>
      {children}
    </div>
  );
}
