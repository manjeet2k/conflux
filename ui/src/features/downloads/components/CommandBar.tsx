// src/features/downloads/components/CommandBar.tsx
import React from 'react';
import {
  Button,
  Menu,
  MenuItem,
  MenuList,
  MenuPopover,
  MenuTrigger,
  MenuDivider,
  Overflow,
  OverflowItem,
  OverflowDivider,
  SearchBox,
  Toolbar,
  ToolbarButton,
  ToolbarDivider,
  Tooltip,
  makeStyles,
  tokens,
  useIsOverflowItemVisible,
  useOverflowMenu,
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
  PanelBottom20Regular,
  Broom20Regular,
} from '@fluentui/react-icons';

const useStyles = makeStyles({
  root: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    // Left inset matches the list's text column so buttons, headers and rows share one edge.
    // 48px of content plus the 1px divider; an odd inner height would center on half pixels.
    height: '49px',
    boxSizing: 'border-box',
    padding: '0 12px',
    borderBottom: '1px solid var(--cfx-divider)',
    flexShrink: 0,
  },
  toolbar: {
    flex: 1,
    minWidth: '32px',
    padding: 0,
    '& > button': { flexShrink: 0, whiteSpace: 'nowrap' },
  },
  search: { flex: '0 1 auto', minWidth: '140px', width: 'clamp(140px, 22vw, 240px)' },
  add: { marginRight: '4px', minWidth: '88px', flexShrink: 0 },
  playback: { minWidth: '104px' },
  details: { flexShrink: 0 },
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
  const actions: DownloadAction[] = [
    {
      id: 'playback',
      label: p.canPause ? 'Pause' : 'Resume',
      icon: p.canPause ? <Pause20Regular /> : <Play20Regular />,
      disabled: !p.canPause && !p.canResume,
      onClick: p.canPause ? p.onPause : p.onResume,
      showLabel: true,
    },
    { id: 'remove', label: 'Remove', hint: 'Remove (Del)', icon: <Delete20Regular />, disabled: !p.canRemove, onClick: p.onRemove },
    { id: 'open', label: 'Open', hint: 'Open (Enter)', icon: <Open20Regular />, disabled: !p.canOpen, onClick: p.onOpen, group: 'files' },
    { id: 'reveal', label: 'Show in folder', icon: <FolderOpen20Regular />, disabled: !p.canReveal, onClick: p.onReveal, group: 'files' },
    { id: 'copy', label: 'Copy download link', icon: <Link20Regular />, disabled: !p.canCopyLink, onClick: p.onCopyLink, group: 'files' },
  ];
  return (
    <div className={styles.root}>
      <Tooltip content="Add download (Ctrl+N)" relationship="description">
        <Button appearance="primary" icon={<Add20Regular />} onClick={p.onAdd} className={styles.add}>
          Add
        </Button>
      </Tooltip>
      <Overflow padding={0} hasHiddenItems>
        <Toolbar aria-label="Download actions" className={styles.toolbar} size="medium">
          {actions.map((action) => (
            <React.Fragment key={action.id}>
              {action.id === 'open' && (
                <OverflowDivider groupId="files">
                  <ToolbarDivider className={styles.divider} />
                </OverflowDivider>
              )}
              <OverflowItem id={action.id} groupId={action.group}>
                <Tooltip content={action.hint ?? action.label} relationship="description">
                  <ToolbarButton
                    className={action.id === 'playback' ? styles.playback : undefined}
                    aria-label={action.label}
                    icon={action.icon}
                    disabled={action.disabled}
                    onClick={action.onClick}
                  >
                    {action.showLabel ? action.label : undefined}
                  </ToolbarButton>
                </Tooltip>
              </OverflowItem>
            </React.Fragment>
          ))}
          <ActionOverflowMenu actions={actions} commands={p} />
        </Toolbar>
      </Overflow>
      <SearchBox
        className={styles.search}
        placeholder="Search downloads"
        value={p.search}
        onChange={(_, d) => p.onSearch(d.value)}
        input={{ ref: p.searchRef }}
        aria-label="Search downloads (Ctrl+F)"
      />
      <Tooltip content="Details pane" relationship="label">
        <Button
          className={styles.details}
          appearance={p.detailsOpen ? 'secondary' : 'subtle'}
          aria-label="Details pane"
          aria-pressed={p.detailsOpen}
          icon={<PanelBottom20Regular />}
          onClick={p.onToggleDetails}
        />
      </Tooltip>
    </div>
  );
};

interface DownloadAction {
  id: string;
  label: string;
  hint?: string;
  icon: React.ReactElement;
  disabled: boolean;
  onClick: () => void;
  showLabel?: boolean;
  group?: string;
}

const OverflowAction: React.FC<{ action: DownloadAction }> = ({ action }) => {
  const visible = useIsOverflowItemVisible(action.id);
  if (visible) return null;
  return (
    <MenuItem icon={action.icon} disabled={action.disabled} onClick={action.onClick}>
      {action.label}
    </MenuItem>
  );
};

const ActionOverflowMenu: React.FC<{ actions: DownloadAction[]; commands: CommandBarProps }> = ({ actions, commands }) => {
  const { ref, overflowCount } = useOverflowMenu<HTMLButtonElement>();
  return (
    <Menu>
      <MenuTrigger disableButtonEnhancement>
        <Tooltip content="More actions" relationship="label">
          <ToolbarButton ref={ref} aria-label="More actions" icon={<MoreHorizontal20Regular />} />
        </Tooltip>
      </MenuTrigger>
      <MenuPopover>
        <MenuList>
          {actions.map((action) => <OverflowAction key={action.id} action={action} />)}
          {overflowCount > 0 && <MenuDivider />}
          <MenuItem icon={<Play20Regular />} disabled={!commands.anyPaused} onClick={commands.onResumeAll}>
            Resume all
          </MenuItem>
          <MenuItem icon={<Pause20Regular />} disabled={!commands.anyActive} onClick={commands.onPauseAll}>
            Pause all
          </MenuItem>
          <MenuDivider />
          <MenuItem icon={<Broom20Regular />} disabled={!commands.anyCompleted} onClick={commands.onClearCompleted}>
            Clear completed from list
          </MenuItem>
        </MenuList>
      </MenuPopover>
    </Menu>
  );
};
