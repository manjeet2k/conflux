import React, { useEffect, useState } from 'react';
import { Button, Dropdown, Option, SpinButton, Spinner, Switch, Text, makeStyles, tokens } from '@fluentui/react-components';
import {
  Alert20Regular,
  DarkTheme20Regular,
  Folder20Regular,
  PlugConnected20Regular,
  PuzzlePiece20Regular,
  Info20Regular,
  Navigation20Regular,
} from '@fluentui/react-icons';
import { open } from '@tauri-apps/plugin-dialog';
import { downloadDir } from '@tauri-apps/api/path';
import { getVersion } from '@tauri-apps/api/app';
import { errorText } from '../api';
import type { Settings, ThemePreference } from '../types';
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

  useEffect(() => {
    downloadDir().then(setOsDownloads).catch(() => {});
    getVersion().then(setVersion).catch(() => {});
  }, []);

  const browse = async () => {
    try {
      const dir = await open({ directory: true, multiple: false, defaultPath: settings.default_save_dir ?? osDownloads });
      if (typeof dir === 'string') onChange({ default_save_dir: dir });
    } catch (e) {
      onError(`Folder picker failed: ${errorText(e)}`);
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

      <SectionHeader>About</SectionHeader>
      <SettingsCard
        icon={<Info20Regular />}
        title="Conflux"
        description="Multi-interface download accelerator: splits files into byte ranges and fetches them over every network adapter at once."
      >
        <Text style={{ color: tokens.colorNeutralForeground3 }}>{version && `Version ${version}`}</Text>
      </SettingsCard>
    </Page>
  );
};
