import React, { useEffect, useState } from 'react';
import { makeStyles, mergeClasses, tokens } from '@fluentui/react-components';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { AppLogo } from './AppLogo';

const useStyles = makeStyles({
  root: {
    height: '40px',
    flexShrink: 0,
    display: 'flex',
    alignItems: 'stretch',
    color: tokens.colorNeutralForeground1,
  },
  drag: {
    flex: 1,
    display: 'flex',
    alignItems: 'center',
    gap: '12px',
    paddingLeft: '16px',
    fontSize: tokens.fontSizeBase200,
    minWidth: 0,
  },
  caption: {
    width: '46px',
    border: 'none',
    background: 'transparent',
    color: tokens.colorNeutralForeground1,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    cursor: 'default',
    ':hover': { backgroundColor: 'var(--cfx-subtle-hover)' },
    ':active': { backgroundColor: 'var(--cfx-subtle-pressed)', color: tokens.colorNeutralForeground2 },
  },
  close: {
    ':hover': { backgroundColor: '#C42B1C', color: '#FFFFFF' },
    ':active': { backgroundColor: '#C83C31', color: 'rgba(255,255,255,0.7)' },
  },
  inactive: { color: tokens.colorNeutralForegroundDisabled },
});

// Glyphs drawn to match Segoe Fluent Icons' ChromeMinimize/Maximize/Restore/Close at 10px.
const MinimizeGlyph = () => (
  <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
    <path d="M0 5h10" stroke="currentColor" strokeWidth="1" />
  </svg>
);
const MaximizeGlyph = () => (
  <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
    <rect x="0.5" y="0.5" width="9" height="9" rx="1" fill="none" stroke="currentColor" />
  </svg>
);
const RestoreGlyph = () => (
  <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
    <rect x="0.5" y="2.5" width="7" height="7" rx="1" fill="none" stroke="currentColor" />
    <path d="M2.5 2.5V1.5a1 1 0 0 1 1-1h5a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1h-1" fill="none" stroke="currentColor" />
  </svg>
);
const CloseGlyph = () => (
  <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
    <path d="M0.5 0.5l9 9M9.5 0.5l-9 9" stroke="currentColor" strokeWidth="1" />
  </svg>
);

export const TitleBar: React.FC = () => {
  const styles = useStyles();
  const [maximized, setMaximized] = useState(false);
  const [focused, setFocused] = useState(true);

  useEffect(() => {
    const win = getCurrentWindow();
    const sync = () => win.isMaximized().then(setMaximized).catch(() => {});
    sync();
    const unResize = win.onResized(sync);
    const unFocus = win.onFocusChanged((e) => setFocused(e.payload));
    return () => {
      unResize.then((fn) => fn());
      unFocus.then((fn) => fn());
    };
  }, []);

  const win = () => getCurrentWindow();

  return (
    <header className={mergeClasses(styles.root, !focused && styles.inactive)}>
      <div data-tauri-drag-region className={styles.drag}>
        <AppLogo size={16} />
        <span data-tauri-drag-region>Conflux</span>
      </div>
      <button className={styles.caption} title="Minimize" aria-label="Minimize" onClick={() => win().minimize()}>
        <MinimizeGlyph />
      </button>
      <button
        className={styles.caption}
        title={maximized ? 'Restore Down' : 'Maximize'}
        aria-label={maximized ? 'Restore Down' : 'Maximize'}
        onClick={() => win().toggleMaximize()}
      >
        {maximized ? <RestoreGlyph /> : <MaximizeGlyph />}
      </button>
      <button
        className={mergeClasses(styles.caption, styles.close)}
        title="Close"
        aria-label="Close"
        onClick={() => win().close()}
      >
        <CloseGlyph />
      </button>
    </header>
  );
};
