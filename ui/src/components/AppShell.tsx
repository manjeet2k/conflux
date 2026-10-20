import React, { type ComponentProps } from 'react';
import { FluentProvider, Toaster, makeStyles, tokens } from '@fluentui/react-components';
import { darkTheme, lightTheme, surfaceVars, windowsControlStyleHooks } from '@/theme';
import { TitleBar } from '@/components/TitleBar';
import { NavPane } from '@/components/NavPane';
import { StatusBar } from '@/components/StatusBar';
import { usePref } from '@/hooks/usePref';
import type { Backdrop } from '@/features/settings/types/settings.types';

const useStyles = makeStyles({
  darkSurfaces: surfaceVars(true, 'none'),
  lightSurfaces: surfaceVars(false, 'none'),
  root: {
    height: '100vh',
    display: 'flex',
    flexDirection: 'column',
    backgroundColor: 'var(--cfx-window)',
    color: tokens.colorNeutralForeground1,
    overflow: 'hidden',
  },
  body: {
    flex: 1,
    display: 'flex',
    minHeight: 0,
  },
  layer: {
    flex: 1,
    minWidth: 0,
    display: 'flex',
    flexDirection: 'column',
    backgroundColor: 'var(--cfx-layer)',
    borderTop: '1px solid var(--cfx-stroke)',
    borderLeft: '1px solid var(--cfx-stroke)',
    borderTopLeftRadius: tokens.borderRadiusXLarge,
    overflow: 'hidden',
  },
  viewContainer: {
    flex: 1,
    minWidth: 0,
    minHeight: 0,
    display: 'flex',
    flexDirection: 'column',
    animationName: 'win11PageEntrance',
    animationDuration: '200ms',
    animationTimingFunction: 'cubic-bezier(0.1, 0.9, 0.2, 1.0)',
    animationFillMode: 'both',
    '@media (prefers-reduced-motion: reduce)': {
      animation: 'none',
    },
  },
});

interface AppShellProps {
  dark: boolean;
  backdrop: Backdrop;
  toasterId: string;
  navigation: Pick<ComponentProps<typeof NavPane>, 'view' | 'onSelect' | 'counts'>;
  status: ComponentProps<typeof StatusBar>;
  children: React.ReactNode;
  dialogs: React.ReactNode;
}

export const AppShell: React.FC<AppShellProps> = ({
  dark, backdrop, toasterId, navigation, status, children, dialogs,
}) => {
  const styles = useStyles();
  const [compactNav, setCompactNav] = usePref('cfx.compactNav', window.innerWidth < 1100);
  const isListView = navigation.view !== 'network' && navigation.view !== 'settings';

  return (
    <FluentProvider
      theme={dark ? darkTheme : lightTheme}
      customStyleHooks_unstable={windowsControlStyleHooks}
      className={dark ? styles.darkSurfaces : styles.lightSurfaces}
      style={{ ...surfaceVars(dark, backdrop), backgroundColor: 'transparent' }}
    >
      <div className={styles.root}>
        <TitleBar />
        <div className={styles.body}>
          <NavPane
            {...navigation}
            compact={compactNav}
            onToggleCompact={() => setCompactNav((compact) => !compact)}
          />
          <main className={styles.layer}>
            <div key={isListView ? 'downloads' : navigation.view} className={styles.viewContainer}>
              {children}
            </div>
            <StatusBar {...status} />
          </main>
        </div>
      </div>
      {dialogs}
      <Toaster toasterId={toasterId} position="bottom-end" offset={{ vertical: 40 }} />
    </FluentProvider>
  );
};
