import React, { useEffect, useState } from 'react';
import { Folder20Regular } from '@fluentui/react-icons';
import { fileKind, fileTypeLabels } from '../utils/fileTypes';
import { fileExtension, shellIconUrl } from '../utils/shellIcons';

function useShellIcon(extension: string | null, folder: boolean) {
  const key = folder ? 'folder' : 'file:' + (extension ?? '');
  const [loaded, setLoaded] = useState<{ key: string; url: string | null } | null>(null);
  useEffect(() => {
    let active = true;
    shellIconUrl(extension, folder).then(url => { if (active) setLoaded({ key, url }); });
    return () => { active = false; };
  }, [extension, folder, key]);
  return loaded?.key === key ? loaded.url : null;
}

const placeholder: React.CSSProperties = {
  display: 'inline-block', width: '0.65em', height: '0.85em', margin: '0.075em 0.175em',
  border: '1px solid currentColor', borderRadius: 2, opacity: 0.3, boxSizing: 'border-box',
};

/** Shell association icons work even when the download's output file does not exist yet. */
export const FileIcon: React.FC<{ filename: string; className?: string }> = React.memo(({ filename, className }) => {
  const kind = fileKind(filename);
  const source = useShellIcon(fileExtension(filename), false);
  return <span className={className} aria-hidden="true" title={fileTypeLabels[kind]} data-file-kind={kind}
    style={{ width: '1em', height: '1em', display: 'inline-flex', flexShrink: 0, verticalAlign: 'middle' }}>
    {source ? <img src={source} alt="" draggable={false} style={{ width: '100%', height: '100%', objectFit: 'contain' }} />
      : <span style={placeholder} />}
  </span>;
});

export const FolderIcon: React.FC<{ className?: string }> = React.memo(({ className }) => {
  const source = useShellIcon(null, true);
  if (source) {
    return (
      <span className={className} aria-hidden="true"
        style={{ width: 20, height: 20, display: 'inline-flex', flexShrink: 0, verticalAlign: 'middle' }}>
        <img src={source} alt="" draggable={false} style={{ width: '100%', height: '100%', objectFit: 'contain' }} />
      </span>
    );
  }
  return <Folder20Regular className={className} aria-hidden="true" />;
});
