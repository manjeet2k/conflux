import React from 'react';
import {
  Button,
  Menu,
  MenuItem,
  MenuList,
  MenuPopover,
  MenuTrigger,
  MenuDivider,
  SearchBox,
  Toolbar,
  ToolbarButton,
  ToolbarDivider,
  ToolbarToggleButton,
  Tooltip,
  makeStyles,
  tokens,
} from '@fluentui/react-components';
import {
  Add20Regular,
  Play20Regular,
  Pause20Regular,
  Delete20Regular,
  Open20Regular,
  FolderOpen20Regular,
  Link20Regular,
  MoreHorizontal20Regular,
  PanelRight20Regular,
  Broom20Regular,
} from '@fluentui/react-icons';

const useStyles = makeStyles({
  root: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    padding: '6px 12px 6px 8px',
    borderBottom: '1px solid var(--cfx-divider)',
    flexShrink: 0,
  },
  toolbar: { flex: 1, minWidth: 0, overflow: 'hidden' },
  search: { width: '240px' },
  add: { marginRight: '4px' },
  divider: { color: tokens.colorNeutralStroke2 },
});

export interface CommandState {
  canResume: boolean;
  canPause: boolean;
  canRemove: boolean;
  canOpen: boolean;
  canReveal: boolean;
  canCopyLink: boolean;
  anyPaused: boolean;
  anyActive: boolean;
  anyCompleted: boolean;
}

interface CommandBarProps extends CommandState {
  onAdd: () => void;
  onResume: () => void;
  onPause: () => void;
  onRemove: () => void;
  onOpen: () => void;
  onReveal: () => void;
  onCopyLink: () => void;
  onResumeAll: () => void;
  onPauseAll: () => void;
  onClearCompleted: () => void;
  search: string;
  onSearch: (value: string) => void;
  searchRef: React.Ref<HTMLInputElement>;
  detailsOpen: boolean;
  onToggleDetails: () => void;
}

export const CommandBar: React.FC<CommandBarProps> = (p) => {
  const styles = useStyles();
  return (
    <div className={styles.root}>
      <Tooltip content="Add download (Ctrl+N)" relationship="description">
        <Button appearance="primary" icon={<Add20Regular />} onClick={p.onAdd} className={styles.add}>
          Add
        </Button>
      </Tooltip>
      <Toolbar aria-label="Download actions" className={styles.toolbar} size="medium">
        <ToolbarButton icon={<Play20Regular />} disabled={!p.canResume} onClick={p.onResume}>
          Resume
        </ToolbarButton>
        <ToolbarButton icon={<Pause20Regular />} disabled={!p.canPause} onClick={p.onPause}>
          Pause
        </ToolbarButton>
        <Tooltip content="Remove (Del)" relationship="description">
          <ToolbarButton
            aria-label="Remove"
            icon={<Delete20Regular />}
            disabled={!p.canRemove}
            onClick={p.onRemove}
          />
        </Tooltip>
        <ToolbarDivider className={styles.divider} />
        <Tooltip content="Open (Enter)" relationship="description">
          <ToolbarButton aria-label="Open" icon={<Open20Regular />} disabled={!p.canOpen} onClick={p.onOpen} />
        </Tooltip>
        <Tooltip content="Show in folder" relationship="description">
          <ToolbarButton
            aria-label="Show in folder"
            icon={<FolderOpen20Regular />}
            disabled={!p.canReveal}
            onClick={p.onReveal}
          />
        </Tooltip>
        <Tooltip content="Copy download link" relationship="description">
          <ToolbarButton
            aria-label="Copy download link"
            icon={<Link20Regular />}
            disabled={!p.canCopyLink}
            onClick={p.onCopyLink}
          />
        </Tooltip>
        <Menu>
          <MenuTrigger disableButtonEnhancement>
            <Tooltip content="More options" relationship="label">
              <ToolbarButton icon={<MoreHorizontal20Regular />} />
            </Tooltip>
          </MenuTrigger>
          <MenuPopover>
            <MenuList>
              <MenuItem icon={<Play20Regular />} disabled={!p.anyPaused} onClick={p.onResumeAll}>
                Resume all
              </MenuItem>
              <MenuItem icon={<Pause20Regular />} disabled={!p.anyActive} onClick={p.onPauseAll}>
                Pause all
              </MenuItem>
              <MenuDivider />
              <MenuItem icon={<Broom20Regular />} disabled={!p.anyCompleted} onClick={p.onClearCompleted}>
                Clear completed from list
              </MenuItem>
            </MenuList>
          </MenuPopover>
        </Menu>
      </Toolbar>
      <SearchBox
        className={styles.search}
        placeholder="Search downloads"
        value={p.search}
        onChange={(_, d) => p.onSearch(d.value)}
        input={{ ref: p.searchRef }}
        aria-label="Search downloads (Ctrl+F)"
      />
      <Tooltip content="Details pane" relationship="label">
        <ToolbarToggleButtonStandalone checked={p.detailsOpen} onClick={p.onToggleDetails} />
      </Tooltip>
    </div>
  );
};

/** ToolbarToggleButton needs a Toolbar context; this is its standalone equivalent. */
const ToolbarToggleButtonStandalone: React.FC<{ checked: boolean; onClick: () => void }> = ({ checked, onClick }) => (
  <Toolbar checkedValues={{ panel: checked ? ['details'] : [] }} onCheckedValueChange={onClick}>
    <ToolbarToggleButton
      aria-label="Details pane"
      appearance="subtle"
      icon={<PanelRight20Regular />}
      name="panel"
      value="details"
    />
  </Toolbar>
);
