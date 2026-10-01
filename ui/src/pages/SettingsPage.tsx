import React, { useEffect, useState } from 'react';
import { Badge, Button, Dropdown, Option, SpinButton, Spinner, Switch, Text, makeStyles, tokens } from '@fluentui/react-components';
import {
  Alert20Regular,
  DarkTheme20Regular,
  Folder20Regular,
  PlugConnected20Regular,
  PuzzlePiece20Regular,
  Info20Regular,
  Navigation20Regular,
  Warning20Regular,
  Bug20Regular,
  ArrowSync20Regular,
  Globe20Regular,
} from '@fluentui/react-icons';
import { open } from '@tauri-apps/plugin-dialog';
import { downloadDir } from '@tauri-apps/api/path';
import { getVersion } from '@tauri-apps/api/app';
import { api, errorText } from '../api';
import type { Settings, ThemePreference, UpdateInfo } from '../types';
import { Page, SectionHeader, SettingsCard } from '../components/Page';

const useStyles = makeStyles({
  stack: { display: 'flex', flexDirection: 'column', gap: '4px' },
  path: {
    maxWidth: '320px',
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    whiteSpace: 'nowrap',
    color: tokens.colorNeutralForeground2,
  },
  dropdown: { minWidth: '160px' },
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
});

const themeLabels: Record<ThemePreference, string> = { system: 'Use system setting', light: 'Light', dark: 'Dark' };
const chunkSizes = [1, 2, 4, 8, 16, 32, 64];

interface SettingsPageProps {
  settings: Settings;
  /** False until the backend's settings arrive; edits before then would save defaults. */
  loaded: boolean;
  onChange: (patch: Partial<Settings>) => void;
  onError: (message: string) => void;
}

export const SettingsPage: React.FC<SettingsPageProps> = ({ settings, loaded, onChange, onError }) => {
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

  useEffect(() => {
    downloadDir().then(setOsDownloads).catch(() => {});
    getVersion().then(setVersion).catch(() => {});
    api
      .getBrowserIntegrationStatus()
      .then((s) => setBrowserHostRegistered(s.registered))
      .catch(() => setBrowserHostRegistered(false));
  }, []);

  // Warn when the chosen default folder is gone (e.g. an unplugged drive).
  useEffect(() => {
    const dir = settings.default_save_dir;
    if (!dir) return;
    let current = true;
    api
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
      const found = await api.checkForUpdate();
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
      await api.installUpdate();
    } catch (e) {
      onError(errorText(e));
    } finally {
      setUpdateBusy('idle');
    }
  };

  const openAbout = (target: 'licenses' | 'repo' | 'releases') => {
    api.openAboutLink(target).catch((e) => onError(errorText(e)));
  };

  const copyDiagnostics = async () => {
    try {
      const diagnostics = await api.getDiagnostics();
      await navigator.clipboard.writeText(JSON.stringify(diagnostics, null, 2));
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2500);
    } catch (e) {
      onError(`Couldn't copy diagnostics: ${errorText(e)}`);
    }
  };

  const openLogs = () => {
    api.openLogsFolder().catch((e) => onError(`Couldn't open the log folder: ${errorText(e)}`));
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
      await api.registerBrowserExtension();
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
      <SettingsCard icon={<DarkTheme20Regular />} title="App theme" description="Choose light or dark, or follow Windows">
        <Dropdown
          className={styles.dropdown}
          value={themeLabels[settings.theme]}
          selectedOptions={[settings.theme]}
          onOptionSelect={(_, d) => onChange({ theme: d.optionValue as ThemePreference })}
        >
          {(Object.keys(themeLabels) as ThemePreference[]).map((t) => (
            <Option key={t} value={t}>
              {themeLabels[t]}
            </Option>
          ))}
        </Dropdown>
      </SettingsCard>

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
          icon={<PuzzlePiece20Regular />}
          title="Chunk size"
          description="Size of each byte-range request. Smaller chunks balance slow adapters better; larger ones mean fewer requests."
        >
          <Dropdown
            className={styles.dropdown}
            value={`${settings.chunk_size_mb} MB`}
            selectedOptions={[String(settings.chunk_size_mb)]}
            onOptionSelect={(_, d) => onChange({ chunk_size_mb: Number(d.optionValue) })}
          >
            {chunkSizes.map((mb) => (
              <Option key={mb} value={String(mb)}>
                {`${mb} MB`}
              </Option>
            ))}
          </Dropdown>
        </SettingsCard>
        <Text size={200} style={{ color: tokens.colorNeutralForeground3, padding: '4px 2px' }}>
          Connection and chunk settings apply to downloads started or resumed afterwards. A resumed download keeps
          its original chunk size.
        </Text>
      </div>

      <SectionHeader>Application</SectionHeader>
      <div className={styles.stack}>
        <SettingsCard
          icon={<PlugConnected20Regular />}
          title="Auto-aggregate new connections"
          description="Automatically detect and utilize newly connected network interfaces (Wi-Fi, Ethernet, USB mobile tethering) in active downloads"
        >
          <Switch
            checked={settings.auto_aggregate_adapters}
            onChange={(_, d) => onChange({ auto_aggregate_adapters: d.checked })}
            label={settings.auto_aggregate_adapters ? 'On' : 'Off'}
            labelPosition="before"
          />
        </SettingsCard>
        <SettingsCard
          icon={<Navigation20Regular />}
          title="Close to system tray"
          description="Keep Conflux running in the notification area and continue downloading when the window is closed"
        >
          <Switch
            checked={settings.close_to_tray}
            onChange={(_, d) => onChange({ close_to_tray: d.checked })}
            label={settings.close_to_tray ? 'On' : 'Off'}
            labelPosition="before"
          />
        </SettingsCard>
      </div>

      <SectionHeader>Notifications</SectionHeader>
      <SettingsCard
        icon={<Alert20Regular />}
        title="Notify when a download finishes"
        description="Show a Windows notification when a download completes or fails"
      >
        <Switch
          checked={settings.notify_on_complete}
          onChange={(_, d) => onChange({ notify_on_complete: d.checked })}
          label={settings.notify_on_complete ? 'On' : 'Off'}
          labelPosition="before"
        />
      </SettingsCard>

      <SettingsCard
        icon={<ArrowSync20Regular />}
        title="Check for updates on start"
        description="Look for a new version shortly after Conflux starts. It only tells you; it never installs by itself. Contacts github.com and sends nothing else."
      >
        <Switch
          checked={settings.check_updates_on_start}
          onChange={(_, d) => onChange({ check_updates_on_start: d.checked })}
          label={settings.check_updates_on_start ? 'On' : 'Off'}
          labelPosition="before"
        />
      </SettingsCard>

      <SectionHeader id="settings-browser">Browser Integration</SectionHeader>
      <div className={styles.stack}>
        <SettingsCard
          icon={<Globe20Regular />}
          title="Browser extension"
          description={
            browserHostRegistered
              ? 'Conflux native messaging host is registered. Install or load the browser extension in Chrome, Edge, or Firefox to capture downloads.'
              : 'Conflux native messaging host is not registered. Register the host to enable browser extension communication.'
          }
        >
          <div className={styles.buttons}>
            {browserHostRegistered !== null && (
              <Badge
                appearance="tint"
                color={browserHostRegistered ? 'success' : 'warning'}
                style={{ alignSelf: 'center', marginRight: '4px' }}
              >
                {browserHostRegistered ? 'Host Registered' : 'Not Registered'}
              </Badge>
            )}
            <Button appearance="primary" onClick={() => api.openAboutLink('extension')}>
              Setup guide
            </Button>
            <Button onClick={handleRegisterHost} disabled={registeringHost}>
              {registeringHost
                ? 'Registering...'
                : browserHostRegistered
                  ? 'Re-register host'
                  : 'Register host'}
            </Button>
          </div>
        </SettingsCard>
      </div>

      <SectionHeader>Support</SectionHeader>
      <SettingsCard
        icon={<Bug20Regular />}
        title="Diagnostics"
        description="Copy app version, system, adapter and error details for a bug report. Contains no file paths, user names, URLs or full IP addresses."
      >
        <div className={styles.buttons}>
          <Button onClick={copyDiagnostics}>{copied ? 'Copied' : 'Copy diagnostics'}</Button>
          <Button onClick={openLogs}>Open logs folder</Button>
        </div>
      </SettingsCard>

      <SectionHeader id="settings-about">About &amp; Updates</SectionHeader>
      <SettingsCard
        icon={<Info20Regular />}
        title="Conflux"
        description="Multi-interface download accelerator: splits files into byte ranges and fetches them over every network adapter at once."
      >
        <Text style={{ color: tokens.colorNeutralForeground3 }}>{version && `Version ${version}`}</Text>
      </SettingsCard>
      <SettingsCard
        icon={<ArrowSync20Regular />}
        title="Updates"
        description={
          update
            ? `Version ${update.version} is available. Running downloads are paused, then resumed after the restart.`
            : upToDate
              ? 'You are running the latest version.'
              : 'Check whether a newer version has been released.'
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
