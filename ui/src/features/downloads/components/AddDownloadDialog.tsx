// src/features/downloads/components/AddDownloadDialog.tsx
import React from 'react';
import {
  Badge,
  Button,
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
import { Folder20Regular, Link20Regular } from '@fluentui/react-icons';
import type { RequestHeaders } from '../types/downloads.types';
import { useAddDownload } from '../hooks/useAddDownload';
import type { StartDownloadParams } from '../utils/downloadValidation';
import { formatBytes } from '@/utils/formatters';
import { FileIcon } from '@/components/FileIcon';

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
  probeIcon: { fontSize: '40px', flexShrink: 0 },
  probeBody: { flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', gap: '6px' },
  probeMeta: { display: 'flex', alignItems: 'center', gap: '8px', flexWrap: 'wrap' },
  folderRow: { display: 'flex', gap: '8px' },
  folderInput: { flex: 1 },
});

interface AddDownloadDialogProps {
  open: boolean;
  requestId: string;
  initialHandoffId?: string | null;
  initialUrl: string;
  initialFilename?: string | null;
  initialHeaders?: RequestHeaders | null;
  defaultSaveDir: string | null;
  queueCount?: number;
  onStart: (args: StartDownloadParams) => Promise<void>;
  onClose: (accepted?: boolean) => void;
  onCancelAll?: () => void;
}

/** Only advancing to a different request resets the form. Queue arrivals retain edits/submission. */
export const AddDownloadDialog: React.FC<AddDownloadDialogProps> = (props) => (
  <Dialog open={props.open} onOpenChange={(_, d) => !d.open && (props.onCancelAll ? props.onCancelAll() : props.onClose())}>
    <AddDownloadForm key={props.requestId} {...props} />
  </Dialog>
);

const AddDownloadForm: React.FC<AddDownloadDialogProps> = ({
  initialUrl,
  initialFilename,
  initialHeaders,
  initialHandoffId,
  defaultSaveDir,
  queueCount = 0,
  onStart,
  onClose,
  onCancelAll,
}) => {
  const styles = useStyles();
  const {
    url, setUrl, filename, setFilename, saveDir, setSaveDir, submitting,
    submitError, effectiveName, canSubmit, probe, probeError, probing, browse, submit,
  } = useAddDownload({
    initialUrl, initialFilename, initialHeaders, initialHandoffId,
    defaultSaveDir, onStart, onClose,
  });

  return (
    <DialogSurface className={styles.surface}>
      <form onSubmit={submit}>
        <DialogBody>
          <DialogTitle
            action={
              queueCount > 0 ? (
                <Badge appearance="tint" color="informative">
                  {queueCount + 1} queued
                </Badge>
              ) : undefined
            }
          >
            Add download
          </DialogTitle>
          <DialogContent className={styles.content}>
            <Field
              label="Address"
              required
              hint="The link is checked automatically after you paste or type it."
            >
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

            {submitError && (
              <MessageBar intent="error" layout="multiline">
                <MessageBarBody style={{ wordBreak: 'break-word', userSelect: 'text' }}>{submitError}</MessageBarBody>
              </MessageBar>
            )}
          </DialogContent>
          <DialogActions>
            {queueCount > 0 && (
              <Button onClick={() => (onCancelAll ?? onClose)()} appearance="subtle">
                Cancel All
              </Button>
            )}
            <Button onClick={() => onClose()}>{queueCount > 0 ? 'Skip' : 'Cancel'}</Button>
            <Button appearance="primary" type="submit" disabled={!canSubmit}>
              {submitting ? <Spinner size="tiny" /> : 'Download'}
            </Button>
          </DialogActions>
        </DialogBody>
      </form>
    </DialogSurface>
  );
};
