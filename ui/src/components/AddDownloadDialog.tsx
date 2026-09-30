import React, { useEffect, useState } from 'react';
import {
  Accordion,
  AccordionHeader,
  AccordionItem,
  AccordionPanel,
  Badge,
  Button,
  Caption1,
  Checkbox,
  Dialog,
  DialogActions,
  DialogBody,
  DialogContent,
  DialogSurface,
  DialogTitle,
  Field,
  Input,
  MessageBar,
  MessageBarBody,
  Spinner,
  Text,
  makeStyles,
  tokens,
} from '@fluentui/react-components';
import { ArrowClockwise16Regular, Folder20Regular, Link20Regular } from '@fluentui/react-icons';
import { open } from '@tauri-apps/plugin-dialog';
import { downloadDir } from '@tauri-apps/api/path';
import { api, errorText } from '../api';
import type { AdapterInfo, ProbeResult } from '../types';
import { formatBytes } from '../utils/formatters';
import { FileIcon } from './FileIcon';
import { kindIcon, usableAdapters } from '../utils/adapters';

const useStyles = makeStyles({
  surface: { width: '560px', maxWidth: 'calc(100vw - 48px)' },
  content: { display: 'flex', flexDirection: 'column', gap: '16px', paddingTop: '4px' },
  probe: {
    display: 'flex',
    gap: '12px',
    alignItems: 'center',
    padding: '12px',
    borderRadius: tokens.borderRadiusLarge,
    backgroundColor: tokens.colorNeutralBackground2,
    border: `1px solid ${tokens.colorNeutralStroke2}`,
  },
  probeIcon: { fontSize: '32px', color: tokens.colorNeutralForeground2, flexShrink: 0 },
  probeBody: { flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', gap: '6px' },
  probeMeta: { display: 'flex', alignItems: 'center', gap: '8px', flexWrap: 'wrap' },
  folderRow: { display: 'flex', gap: '8px' },
  folderInput: { flex: 1 },
  adapterList: { display: 'flex', flexDirection: 'column', gap: '2px' },
  adapterLabel: { display: 'inline-flex', alignItems: 'center', gap: '8px' },
  muted: { color: tokens.colorNeutralForeground3 },
  accordionHeader: { display: 'flex', justifyContent: 'space-between', width: '100%' },
});

const isHttpUrl = (s: string) => {
  try {
    const u = new URL(s.trim());
    return u.protocol === 'http:' || u.protocol === 'https:';
  } catch {
    return false;
  }
};

interface AddDownloadDialogProps {
  open: boolean;
  initialUrl: string;
  adapters: AdapterInfo[];
  defaultSaveDir: string | null;
  onRefreshAdapters: () => void;
  onStart: (args: { url: string; saveDir: string; filename: string | null; adapterIds: string[] }) => Promise<void>;
  onClose: () => void;
}

/** Keyed on open state so every open starts from a fresh form. */
export const AddDownloadDialog: React.FC<AddDownloadDialogProps> = (props) => (
  <Dialog open={props.open} onOpenChange={(_, d) => !d.open && props.onClose()}>
    <AddDownloadForm key={`${props.open}:${props.initialUrl}`} {...props} />
  </Dialog>
);

const AddDownloadForm: React.FC<AddDownloadDialogProps> = ({
  initialUrl,
  adapters,
  defaultSaveDir,
  onRefreshAdapters,
  onStart,
  onClose,
}) => {
  const styles = useStyles();
  const [url, setUrl] = useState(initialUrl);
  // Outcome of the last finished probe, tagged with the URL it was for.
  const [probeState, setProbeState] = useState<{ url: string; result?: ProbeResult; error?: string } | null>(null);
  // null = not edited by the user -> follow the probe's suggestion.
  const [filename, setFilename] = useState<string | null>(null);
  const [saveDir, setSaveDir] = useState(defaultSaveDir ?? '');
  const [userSelection, setUserSelection] = useState<string[] | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  useEffect(() => {
    if (defaultSaveDir) return;
    downloadDir()
      .then((dir) => setSaveDir((cur) => cur || dir))
      .catch((e) => console.error('Failed to resolve Downloads folder:', e));
  }, [defaultSaveDir]);

  // Debounced probe: preview name, size and range support before starting. Results for a
  // URL other than the current one are ignored, so stale responses never show.
  const target = url.trim();
  const targetValid = isHttpUrl(target);
  useEffect(() => {
    if (!targetValid) return;
    let cancelled = false;
    const timer = window.setTimeout(() => {
      api
        .probeUrl(target)
        .then((result) => !cancelled && setProbeState({ url: target, result }))
        .catch((e) => !cancelled && setProbeState({ url: target, error: errorText(e) }));
    }, 450);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [target, targetValid]);

  const current = probeState?.url === target ? probeState : null;
  const probe = current?.result ?? null;
  const probeError = current?.error ?? null;
  const probing = targetValid && !current;

  const usable = usableAdapters(adapters);
  const usableIds = new Set(usable.map((a) => a.id));
  const selected =
    userSelection === null
      ? usable.filter((a) => a.enabled).map((a) => a.id)
      : userSelection.filter((id) => usableIds.has(id));

  const toggle = (id: string, checked: boolean) =>
    setUserSelection(checked ? [...selected, id] : selected.filter((s) => s !== id));

  const browse = async () => {
    try {
      const dir = await open({ directory: true, multiple: false, defaultPath: saveDir || undefined });
      if (typeof dir === 'string') setSaveDir(dir);
    } catch (e) {
      console.error('Folder picker failed:', e);
    }
  };

  const effectiveName = (filename ?? probe?.filename ?? '').trim();
  const noAdapter = usable.length > 0 && selected.length === 0;
  const canSubmit = isHttpUrl(url) && saveDir.trim() !== '' && !noAdapter && !submitting;

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!canSubmit) return;
    setSubmitting(true);
    setSubmitError(null);
    try {
      await onStart({
        url: url.trim(),
        saveDir: saveDir.trim(),
        filename: filename !== null && effectiveName ? effectiveName : null,
        adapterIds: selected,
      });
      onClose();
    } catch (err) {
      setSubmitError(errorText(err));
      setSubmitting(false);
    }
  };

  return (
    <DialogSurface className={styles.surface}>
      <form onSubmit={submit}>
        <DialogBody>
          <DialogTitle>Add download</DialogTitle>
          <DialogContent className={styles.content}>
            <Field label="Address" required>
              <Input
                autoFocus
                contentBefore={<Link20Regular />}
                placeholder="https://example.com/file.iso"
                value={url}
                onChange={(_, d) => setUrl(d.value)}
                spellCheck={false}
              />
            </Field>

            {probing && <Spinner size="tiny" label="Checking link…" labelPosition="after" />}
            {probeError && (
              <MessageBar intent="warning" layout="multiline">
                <MessageBarBody style={{ wordBreak: 'break-word' }}>
                  Couldn't check this link: {probeError}
                </MessageBarBody>
              </MessageBar>
            )}
            {probe && (
              <div className={styles.probe}>
                <FileIcon filename={effectiveName} className={styles.probeIcon} />
                <div className={styles.probeBody}>
                  <Input
                    aria-label="File name"
                    value={filename ?? probe.filename}
                    onChange={(_, d) => setFilename(d.value)}
                    appearance="underline"
                    spellCheck={false}
                  />
                  <div className={styles.probeMeta}>
                    <Text size={200}>{probe.total_bytes > 0 ? formatBytes(probe.total_bytes, 2) : 'Unknown size'}</Text>
                    {probe.supports_ranges ? (
                      <Badge appearance="tint" color="success">
                        Resumable · multi-connection
                      </Badge>
                    ) : (
                      <Badge appearance="tint" color="warning">
                        Single connection · can't resume
                      </Badge>
                    )}
                  </div>
                </div>
              </div>
            )}

            <Field label="Save to" required>
              <div className={styles.folderRow}>
                <Input
                  className={styles.folderInput}
                  value={saveDir}
                  onChange={(_, d) => setSaveDir(d.value)}
                  placeholder="Choose a folder"
                  spellCheck={false}
                />
                <Button icon={<Folder20Regular />} onClick={browse}>
                  Browse
                </Button>
              </div>
            </Field>

            <Accordion collapsible defaultOpenItems={noAdapter ? ['adapters'] : []}>
              <AccordionItem value="adapters">
                <AccordionHeader expandIconPosition="end" size="small">
                  Network adapters
                  <Caption1 className={styles.muted} style={{ marginLeft: 8 }}>
                    {usable.length === 0
                      ? 'default route'
                      : `${selected.length} of ${usable.length} selected`}
                  </Caption1>
                </AccordionHeader>
                <AccordionPanel>
                  <div className={styles.adapterList}>
                    {usable.length === 0 && (
                      <Caption1 className={styles.muted}>
                        No bindable IPv4 adapters were found; the system's default route will be used.
                      </Caption1>
                    )}
                    {usable.map((a) => {
                      const Icon = kindIcon[a.kind];
                      return (
                        <Checkbox
                          key={a.id}
                          checked={selected.includes(a.id)}
                          onChange={(_, d) => toggle(a.id, !!d.checked)}
                          label={
                            <span className={styles.adapterLabel}>
                              <Icon />
                              {a.name}
                              <Caption1 className={styles.muted}>{a.ip}</Caption1>
                            </span>
                          }
                        />
                      );
                    })}
                    {noAdapter && (
                      <Caption1 style={{ color: tokens.colorPaletteRedForeground1 }}>
                        Select at least one adapter.
                      </Caption1>
                    )}
                    <div>
                      <Button size="small" appearance="subtle" icon={<ArrowClockwise16Regular />} onClick={onRefreshAdapters}>
                        Refresh adapters
                      </Button>
                    </div>
                  </div>
                </AccordionPanel>
              </AccordionItem>
            </Accordion>

            {submitError && (
              <MessageBar intent="error" layout="multiline">
                <MessageBarBody style={{ wordBreak: 'break-word', userSelect: 'text' }}>{submitError}</MessageBarBody>
              </MessageBar>
            )}
          </DialogContent>
          <DialogActions>
            <Button appearance="primary" type="submit" disabled={!canSubmit}>
              {submitting ? <Spinner size="tiny" /> : 'Download'}
            </Button>
            <Button onClick={onClose}>Cancel</Button>
          </DialogActions>
        </DialogBody>
      </form>
    </DialogSurface>
  );
};
