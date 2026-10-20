import { useCallback, useEffect } from 'react';
import { useId, useToastController, type ToastIntent } from '@fluentui/react-components';
import { settingsApi } from '@/features/settings/api/settings.api';
import type { UpdateInfo } from '@/features/settings/types/settings.types';
import { AppToast } from '../components/AppToast';

export function useAppNotifications(onSettings: () => void, openAbout: () => void) {
  const toasterId = useId('toaster');
  const { dispatchToast } = useToastController(toasterId);
  const notify = useCallback((intent: ToastIntent, title: string) => {
    dispatchToast(<AppToast title={title} />, {
      intent, timeout: intent === 'error' || intent === 'warning' ? 8000 : 3000,
    });
  }, [dispatchToast]);
  const onError = useCallback((message: string) => notify('error', message), [notify]);

  useEffect(() => {
    settingsApi.takeStartupNotices()
      .then((notices) => notices.forEach((notice) => notify('warning', notice)))
      .catch((error) => console.error('Failed to read startup notices:', error));
  }, [notify]);

  useEffect(() => {
    const key = 'cfx.announced.browser-0.2.0';
    if (localStorage.getItem(key)) return;
    localStorage.setItem(key, '1');
    const timer = window.setTimeout(() => {
      dispatchToast(
        <AppToast
          title="Browser integration available" onView={onSettings}
          subtitle="Open Settings → Browser Integration to get the extension."
        >
          Capture downloads directly from Chrome, Edge, and Firefox.
        </AppToast>,
        { intent: 'info', timeout: 12000 }
      );
    }, 1500);
    return () => window.clearTimeout(timer);
  }, [dispatchToast, onSettings]);

  const notifyUpdate = useCallback((info: UpdateInfo) => {
    dispatchToast(
      <AppToast
        title="Update available" onView={openAbout}
        subtitle="Open Settings → About & Updates to install it."
      >
        Conflux {info.version} is ready to download.
      </AppToast>,
      { intent: 'info', timeout: 15000 }
    );
  }, [dispatchToast, openAbout]);

  return { toasterId, notify, onError, notifyUpdate };
}
