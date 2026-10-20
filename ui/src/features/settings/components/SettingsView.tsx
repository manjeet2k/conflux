// src/features/settings/components/SettingsView.tsx
import React, { useEffect, useState } from 'react';
import { Badge, Button, Input, SpinButton, Spinner, Switch, Text, makeStyles, tokens } from '@fluentui/react-components';
import {
  Alert20Regular,
  Blur20Regular,
  DarkTheme20Regular,
  Folder20Regular,
  PlugConnected20Regular,
  Info20Regular,
  Navigation20Regular,
  Warning20Regular,
  Bug20Regular,
  ArrowSync20Regular,
  ArrowDownload20Regular,
} from '@fluentui/react-icons';
import { open } from '@tauri-apps/plugin-dialog';
import { downloadDir } from '@tauri-apps/api/path';
import { getVersion } from '@tauri-apps/api/app';
import { settingsApi } from '../api/settings.api';
import { networkApi } from '@/features/network/api/network.api';
import { errorText } from '@/utils/errors';
import type { Backdrop, Settings, ThemePreference, UpdateInfo } from '../types/settings.types';
import { Page, SectionHeader, SettingsCard } from '@/components/Page';
import { WindowsDropdown } from '@/components/WindowsDropdown';
import { TurtleIcon } from '@/components/TurtleIcon';
import chromeLogo from '@/assets/browsers/chrome.svg';
import firefoxLogo from '@/assets/browsers/firefox.svg';

const useStyles = makeStyles({
  stack: { display: 'flex', flexDirection: 'column', gap: '4px' },
  path: {
    maxWidth: '320px',
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    whiteSpace: 'nowrap',
    color: tokens.colorNeutralForeground2,
  },
  warning: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    padding: '8px 12px',
    color: tokens.colorPaletteDarkOrangeForeground1,
  },
  buttons: { display: 'flex', gap: '8px', flexWrap: 'wrap' },
  notes: {
    margin: 0,
    padding: '8px 12px',
    maxHeight: '160px',
    overflow: 'auto',
    whiteSpace: 'pre-wrap',
    fontFamily: 'inherit',
    fontSize: tokens.fontSizeBase200,
    color: tokens.colorNeutralForeground2,
    backgroundColor: tokens.colorNeutralBackground3,
    borderRadius: tokens.borderRadiusMedium,
  },
  browserIcon: {
    width: '24px',
    height: '24px',
    display: 'block',
    userSelect: 'none',
  },
});

const themeLabels: Record<ThemePreference, string> = { system: 'Use system setting', light: 'Light', dark: 'Dark' };
const backdropLabels: Record<Backdrop, string> = {
  mica_alt: 'Mica Alt',
  mica: 'Mica',
  acrylic: 'Acrylic',
  none: 'None',
};

export interface SettingsViewProps {
  settings: Settings;
  /** False until the backend's settings arrive; edits before then would save defaults. */
  loaded: boolean;
  onChange: (patch: Partial<Settings>) => void;
  onError: (message: string) => void;
}

export const SettingsView: React.FC<SettingsViewProps> = ({ settings, loaded, onChange, onError }) => {
  const styles = useStyles();
  const [osDownloads, setOsDownloads] = useState('');
  const [version, setVersion] = useState('');
  // The folder last found missing; the warning shows only while it is still the chosen one.
  const [missingDir, setMissingDir] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [updateBusy, setUpdateBusy] = useState<'idle' | 'checking' | 'installing'>('idle');
  // Result of the last check: `update` when a newer version exists, `upToDate` when it does not.
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [upToDate, setUpToDate] = useState(false);
  const [browserHostRegistered, setBrowserHostRegistered] = useState<boolean | null>(null);
  const [registeringHost, setRegisteringHost] = useState(false);
  const [editingMbStr, setEditingMbStr] = useState<string | null>(null);
  const currentMb = Math.round((settings.slow_mode_limit_kib / 1024) * 100) / 100;
  const slowModeMbStr = editingMbStr ?? String(currentMb);

  const handleSlowModeMbChange = (raw: string) => {
    setEditingMbStr(raw);
    const parsed = parseFloat(raw);
    if (Number.isFinite(parsed) && parsed > 0) {
      const kib = Math.max(16, Math.min(1024 * 1024, Math.round(parsed * 1024)));
      onChange({ slow_mode_limit_kib: kib });
    }
  };

  const handleSlowModeMbBlur = () => {
    if (editingMbStr !== null) {
      const parsed = parseFloat(editingMbStr);
      if (Number.isFinite(parsed) && parsed > 0) {
        const clampedMb = Math.max(0.02, Math.min(1024, parsed));
        const kib = Math.max(16, Math.min(1024 * 1024, Math.round(clampedMb * 1024)));
        onChange({ slow_mode_limit_kib: kib });
      }
      setEditingMbStr(null);
    }
  };

  useEffect(() => {
    let current = true;
    downloadDir().then((d) => current && setOsDownloads(d)).catch(() => {});
    getVersion().then((v) => current && setVersion(v)).catch(() => {});
    settingsApi
      .getBrowserIntegrationStatus()
      .then((s) => current && setBrowserHostRegistered(s.registered))
      .catch(() => current && setBrowserHostRegistered(false));
    return () => {
      current = false;
    };
  }, []);

  // Warn when the chosen default folder is gone (e.g. an unplugged drive).
  useEffect(() => {
    const dir = settings.default_save_dir;
    if (!dir) return;
    let current = true;
    settingsApi
      .folderExists(dir)
      .then((exists) => current && setMissingDir(exists ? null : dir))
      .catch(() => current && setMissingDir(null));
    return () => {
      current = false;
    };
  }, [settings.default_save_dir]);
  const folderMissing = settings.default_save_dir !== null && missingDir === settings.default_save_dir;

  const checkForUpdate = async () => {
    setUpdateBusy('checking');
    setUpToDate(false);
    try {
      const found = await settingsApi.checkForUpdate();
      setUpdate(found);
      setUpToDate(found === null);
    } catch (e) {
      onError(errorText(e));
    } finally {
      setUpdateBusy('idle');
    }
  };

  // Resolves only if installing failed: on success the installer closes and restarts the app.
  const installUpdate = async () => {
    setUpdateBusy('installing');
    try {
      await settingsApi.installUpdate();
    } catch (e) {
      onError(errorText(e));
    } finally {
      setUpdateBusy('idle');
    }
  };

  const openAbout = (target: 'licenses' | 'repo' | 'releases') => {
    settingsApi.openAboutLink(target).catch((e) => onError(errorText(e)));
  };

  const copyDiagnostics = async () => {
    try {
      const diagnostics = await networkApi.getDiagnostics();
      await navigator.clipboard.writeText(JSON.stringify(diagnostics, null, 2));
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2500);
    } catch (e) {
      onError(`Couldn't copy diagnostics: ${errorText(e)}`);
    }
  };

  const openLogs = () => {
    networkApi.openLogsFolder().catch((e) => onError(`Couldn't open the log folder: ${errorText(e)}`));
  };

  const browse = async () => {
    try {
      const dir = await open({ directory: true, multiple: false, defaultPath: settings.default_save_dir ?? osDownloads });
      if (typeof dir === 'string') onChange({ default_save_dir: dir });
    } catch (e) {
      onError(`Folder picker failed: ${errorText(e)}`);
    }
  };

  const handleRegisterHost = async () => {
    setRegisteringHost(true);
    try {
      await settingsApi.registerBrowserExtension();
      setBrowserHostRegistered(true);
    } catch (e) {
      onError(`Failed to register browser host: ${errorText(e)}`);
    } finally {
      setRegisteringHost(false);
    }
  };

  const folder = settings.default_save_dir ?? osDownloads;

  if (!loaded) {
    return (
      <Page title="Settings">
        <Spinner size="small" label="Loading settings…" labelPosition="after" />
      </Page>
    );
  }

  return (
    <Page title="Settings">
      <SectionHeader>Appearance</SectionHeader>
      <div className={styles.stack}>
        <SettingsCard icon={<DarkTheme20Regular />} title="App theme" description="Choose light or dark, or follow Windows">
          <WindowsDropdown
            label="App theme"
            value={settings.theme}
            options={themeLabels}
            onChange={(theme) => onChange({ theme })}
            width={166}
          />
        </SettingsCard>
        <SettingsCard
          icon={<Blur20Regular />}
          title="Window material"
          description="Let your wallpaper show through the window. Needs Windows 11; Mica Alt and Acrylic need version 22H2 or later."
        >
          <WindowsDropdown
            label="Window material"
            value={settings.backdrop}
            options={backdropLabels}
            onChange={(backdrop) => onChange({ backdrop })}
          />
        </SettingsCard>
      </div>

      <SectionHeader>Downloads</SectionHeader>
      <div className={styles.stack}>
        <SettingsCard
          icon={<Folder20Regular />}
          title="Default download folder"
          description={
            <span className={styles.path} title={folder}>
              {folder || 'Downloads'}
            </span>
          }
        >
          {settings.default_save_dir && (
            <Button appearance="subtle" onClick={() => onChange({ default_save_dir: null })}>
              Reset
            </Button>
          )}
          <Button onClick={browse}>Browse</Button>
        </SettingsCard>
        {folderMissing && (
          <div className={styles.warning} role="alert">
            <Warning20Regular />
            <Text size={200}>
              This folder no longer exists (is the drive connected?). Downloads will fail until you pick another
              folder or reset it.
            </Text>
          </div>
        )}
        <SettingsCard
          icon={<PlugConnected20Regular />}
          title="Connections per adapter"
          description="Parallel connections opened on each network adapter (1–16). More helps on fast links; some servers limit connections."
        >
          <SpinButton
            aria-label="Connections per adapter"
            value={settings.connections_per_adapter}
            min={1}
            max={16}
            style={{ width: 96 }}
            onChange={(_, d) => {
              const v = d.value ?? parseInt(d.displayValue ?? '', 10);
              if (Number.isFinite(v)) onChange({ connections_per_adapter: Math.min(16, Math.max(1, Math.round(v))) });
            }}
          />
        </SettingsCard>
        <SettingsCard
          icon={<TurtleIcon active={false} width={24} height={20} />}
          title="Slow mode speed limit"
          description="Maximum download speed across all active transfers when slow mode is active"
        >
          <Input
            type="number"
            min={0.1}
            max={1024}
            step={0.5}
            value={slowModeMbStr}
            onChange={(_, d) => handleSlowModeMbChange(d.value)}
            onBlur={handleSlowModeMbBlur}
            contentAfter="MB/s"
            aria-label="Slow mode speed limit in MB/s"
            style={{ width: 140 }}
          />
        </SettingsCard>
        <SettingsCard
          icon={<Alert20Regular />}
          title="Notifications"
          description="Windows notification when a download finishes"
        >
          <Switch
            aria-label="Notifications"
            checked={settings.notify_on_complete}
            onChange={(_, d) => onChange({ notify_on_complete: d.checked })}
          />
        </SettingsCard>
        <SettingsCard
          icon={<Navigation20Regular />}
          title="Close button"
          description="Keep Conflux running in the notification area when the window is closed"
        >
          <Switch aria-label="Close to system tray" checked={settings.close_to_tray} onChange={(_, d) => onChange({ close_to_tray: d.checked })} />
        </SettingsCard>
      </div>

      <SectionHeader>Network</SectionHeader>
      <div className={styles.stack}>
        <SettingsCard
          icon={<PlugConnected20Regular />}
          title="Aggregate all connections"
          description="Use every active network adapter together on new downloads"
        >
          <Switch
            aria-label="Aggregate all connections"
            checked={settings.auto_aggregate_adapters}
            onChange={(_, d) => onChange({ auto_aggregate_adapters: d.checked })}
          />
        </SettingsCard>
      </div>

      <SettingsCard
        icon={<ArrowSync20Regular />}
        title="Check for updates on start"
        description="Look for a new version shortly after Conflux starts. It only tells you; it never installs by itself. Contacts github.com and sends nothing else."
      >
        <Switch
          aria-label="Check for updates on start"
          checked={settings.check_updates_on_start}
          onChange={(_, d) => onChange({ check_updates_on_start: d.checked })}
          label={settings.check_updates_on_start ? 'On' : 'Off'}
          labelPosition="before"
        />
      </SettingsCard>

      <SectionHeader id="settings-browser">Browser integration</SectionHeader>
      <div className={styles.stack}>
        <SettingsCard
          icon={<img src={chromeLogo} alt="Chrome" className={styles.browserIcon} />}
          title="Chrome, Edge & Brave"
          description="Intercepts browser downloads and routes them to Conflux"
        >
          <Button onClick={() => settingsApi.openAboutLink('extension-chrome')}>Get extension</Button>
        </SettingsCard>
        <SettingsCard
          icon={<img src={firefoxLogo} alt="Firefox" className={styles.browserIcon} />}
          title="Firefox"
          description="Intercepts browser downloads and routes them to Conflux"
        >
          <Button onClick={() => settingsApi.openAboutLink('extension-firefox')}>Get add-on</Button>
        </SettingsCard>
        <SettingsCard
          icon={<ArrowSync20Regular />}
          title="Native messaging host"
          description={
            browserHostRegistered
              ? 'Registered. Browsers can launch and talk to Conflux.'
              : 'Not registered yet. Required for browser extensions to send downloads to Conflux.'
          }
        >
          {browserHostRegistered && (
            <Badge appearance="tint" color="success">
              Registered
            </Badge>
          )}
          <Button onClick={handleRegisterHost} disabled={registeringHost}>
            {registeringHost ? 'Registering…' : browserHostRegistered ? 'Re-register' : 'Register host'}
          </Button>
        </SettingsCard>
      </div>

      <SectionHeader>Diagnostics & logs</SectionHeader>
      <div className={styles.stack}>
        <SettingsCard
          icon={<Bug20Regular />}
          title="Diagnostics snapshot"
          description="Redacted system details (Windows version, adapters, settings) useful for GitHub issues"
        >
          <Button onClick={copyDiagnostics}>{copied ? 'Copied to clipboard' : 'Copy diagnostics'}</Button>
        </SettingsCard>
        <SettingsCard
          icon={<Folder20Regular />}
          title="Log folder"
          description="Conflux application logs on this PC (rolling 7-day retention)"
        >
          <Button onClick={openLogs}>Open log folder</Button>
        </SettingsCard>
      </div>

      <SectionHeader id="settings-about">About Conflux</SectionHeader>
      <SettingsCard
        icon={<ArrowDownload20Regular />}
        title={version ? `Conflux ${version}` : 'Conflux'}
        description={
          upToDate
            ? "You're on the latest version."
            : update
              ? `Version ${update.version} is available.`
              : 'Multi-interface bandwidth-aggregating download manager for Windows 10 & 11.'
        }
      >
        <div className={styles.buttons}>
          <Button onClick={checkForUpdate} disabled={updateBusy !== 'idle'}>
            {updateBusy === 'checking' ? 'Checking...' : 'Check for updates'}
          </Button>
          {update && (
            <Button appearance="primary" onClick={installUpdate} disabled={updateBusy !== 'idle'}>
              {updateBusy === 'installing' ? 'Installing...' : 'Install and restart'}
            </Button>
          )}
        </div>
      </SettingsCard>
      {update?.notes && <pre className={styles.notes}>{update.notes}</pre>}
      <SettingsCard icon={<Info20Regular />} title="Open source" description="Source code, release history and third-party licenses.">
        <div className={styles.buttons}>
          <Button onClick={() => openAbout('repo')}>Repository</Button>
          <Button onClick={() => openAbout('releases')}>Releases</Button>
          <Button onClick={() => openAbout('licenses')}>Third-party licenses</Button>
        </div>
      </SettingsCard>
    </Page>
  );
};
