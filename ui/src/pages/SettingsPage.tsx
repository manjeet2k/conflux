import React, { useEffect, useState } from 'react';
import { Button, Dropdown, Option, SpinButton, Switch, Text, makeStyles, tokens } from '@fluentui/react-components';
import {
  Alert20Regular,
  DarkTheme20Regular,
  Folder20Regular,
  PlugConnected20Regular,
  PuzzlePiece20Regular,
  Info20Regular,
} from '@fluentui/react-icons';
import { open } from '@tauri-apps/plugin-dialog';
import { downloadDir } from '@tauri-apps/api/path';
import { getVersion } from '@tauri-apps/api/app';
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

export const SettingsPage: React.FC<{ settings: Settings; onChange: (patch: Partial<Settings>) => void }> = ({
  settings,
  onChange,
}) => {
  const styles = useStyles();
  const [osDownloads, setOsDownloads] = useState('');
  const [version, setVersion] = useState('');

  useEffect(() => {
    downloadDir().then(setOsDownloads).catch(() => {});
    getVersion().then(setVersion).catch(() => {});
  }, []);

  const browse = async () => {
    const dir = await open({ directory: true, multiple: false, defaultPath: settings.default_save_dir ?? osDownloads });
    if (typeof dir === 'string') onChange({ default_save_dir: dir });
  };

  const folder = settings.default_save_dir ?? osDownloads;

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
