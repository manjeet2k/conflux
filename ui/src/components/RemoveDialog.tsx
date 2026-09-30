import React, { useState } from 'react';
import {
  Button,
  Checkbox,
  Dialog,
  DialogActions,
  DialogBody,
  DialogContent,
  DialogSurface,
  DialogTitle,
  Text,
} from '@fluentui/react-components';
import type { DownloadTask } from '../types';

interface RemoveDialogProps {
  tasks: DownloadTask[]; // empty = closed
  onConfirm: (ids: string[], deleteFiles: boolean) => void;
  onClose: () => void;
}

export const RemoveDialog: React.FC<RemoveDialogProps> = ({ tasks, onConfirm, onClose }) => (
  <Dialog open={tasks.length > 0} onOpenChange={(_, d) => !d.open && onClose()}>
    <RemoveForm key={tasks.map((t) => t.id).join()} tasks={tasks} onConfirm={onConfirm} onClose={onClose} />
  </Dialog>
);

const RemoveForm: React.FC<RemoveDialogProps> = ({ tasks, onConfirm, onClose }) => {
  const [deleteFiles, setDeleteFiles] = useState(false);
  const completed = tasks.filter((t) => t.status === 'completed').length;
  const unfinished = tasks.length - completed;
  const title = tasks.length === 1 ? `Remove "${tasks[0].filename}"?` : `Remove ${tasks.length} downloads?`;

  return (
    <DialogSurface style={{ maxWidth: 460 }}>
      <DialogBody>
        <DialogTitle style={{ wordBreak: 'break-word' }}>{title}</DialogTitle>
        <DialogContent style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          {unfinished > 0 && (
            <Text>
              {unfinished === 1 && tasks.length === 1
                ? 'This download is unfinished; its partial file will be deleted.'
                : `${unfinished} unfinished download${unfinished === 1 ? '' : 's'} will be stopped and ${unfinished === 1 ? 'its partial file' : 'their partial files'} deleted.`}
            </Text>
          )}
          {completed > 0 && (
            <Checkbox
              checked={deleteFiles}
              onChange={(_, d) => setDeleteFiles(!!d.checked)}
              label={completed === 1 ? 'Also delete the downloaded file from disk' : `Also delete ${completed} downloaded files from disk`}
            />
          )}
        </DialogContent>
        <DialogActions>
          <Button
            appearance="primary"
            onClick={() => {
              onConfirm(
                tasks.map((t) => t.id),
                deleteFiles
              );
              onClose();
            }}
          >
            Remove
          </Button>
          <Button onClick={onClose}>Cancel</Button>
        </DialogActions>
      </DialogBody>
    </DialogSurface>
  );
};
