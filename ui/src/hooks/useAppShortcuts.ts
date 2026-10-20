import { useEffect } from 'react';
import { extractUrl } from '@/utils/files';

const isEditable = (el: EventTarget | null) =>
  el instanceof HTMLElement && (el.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(el.tagName));

interface AppShortcutOptions {
  isDialogOpen: () => boolean;
  onAdd: (url?: string) => void;
  onSearch: () => void;
  onRefresh: () => unknown;
  notify: (intent: 'info', message: string) => void;
}

export function useAppShortcuts({ isDialogOpen, onAdd, onSearch, onRefresh, notify }: AppShortcutOptions) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const key = e.key.toLowerCase();
      const dialogOpen = isDialogOpen();
      if (dialogOpen && e.ctrlKey && (key === 'n' || key === 'f')) {
        e.preventDefault();
      } else if (e.ctrlKey && key === 'n') {
        e.preventDefault();
        onAdd();
      } else if (e.ctrlKey && key === 'f') {
        e.preventDefault();
        onSearch();
      } else if (e.key === 'F5' || (e.ctrlKey && key === 'r')) {
        e.preventDefault();
        onRefresh();
      }
    };
    const onPaste = (e: ClipboardEvent) => {
      if (isDialogOpen() || isEditable(e.target)) return;
      const url = extractUrl(e.clipboardData?.getData('text') ?? '');
      if (url) {
        e.preventDefault();
        onAdd(url);
      } else {
        notify('info', 'The clipboard does not contain a link');
      }
    };
    const onContext = (e: MouseEvent) => {
      if (!isEditable(e.target)) e.preventDefault();
    };
    window.addEventListener('keydown', onKey);
    document.addEventListener('paste', onPaste);
    document.addEventListener('contextmenu', onContext);
    return () => {
      window.removeEventListener('keydown', onKey);
      document.removeEventListener('paste', onPaste);
      document.removeEventListener('contextmenu', onContext);
    };
  }, [isDialogOpen, onAdd, onSearch, onRefresh, notify]);
}
