import type { CSSProperties, MouseEvent as ReactMouseEvent, ReactNode } from 'react';
import { Box } from '@mantine/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { tauriAvailable } from '../../lib/tauri';

interface WindowDragRegionProps {
  children?: ReactNode;
  className?: string;
  style?: CSSProperties;
}

const NON_DRAG_SELECTOR = [
  'a',
  'button',
  'input',
  'select',
  'textarea',
  '[role="button"]',
  '[data-no-window-drag]',
].join(', ');

export function WindowDragRegion({ children, className, style }: WindowDragRegionProps) {
  function handleMouseDown(event: ReactMouseEvent<HTMLDivElement>) {
    if (!tauriAvailable || event.button !== 0 || event.defaultPrevented) return;
    if ((event.target as HTMLElement).closest(NON_DRAG_SELECTOR)) return;

    void getCurrentWindow().startDragging().catch(() => undefined);
  }

  return (
    <Box
      className={className}
      data-tauri-drag-region
      onMouseDown={handleMouseDown}
      style={style}
    >
      {children}
    </Box>
  );
}
