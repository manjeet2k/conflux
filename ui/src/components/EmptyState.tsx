import React from 'react';
import { Body1, Button, Subtitle1, makeStyles, tokens } from '@fluentui/react-components';
import { Add20Regular } from '@fluentui/react-icons';
import type { FluentIcon } from '@fluentui/react-icons';

const useStyles = makeStyles({
  root: {
    flex: 1,
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'center',
    justifyContent: 'center',
    gap: '8px',
    padding: '32px',
    textAlign: 'center',
  },
  icon: { fontSize: '48px', color: tokens.colorNeutralForeground3, marginBottom: '8px' },
  body: { color: tokens.colorNeutralForeground3, maxWidth: '380px' },
  action: { marginTop: '12px' },
});

export const EmptyState: React.FC<{ icon: FluentIcon; title: string; body: string; onAdd?: () => void }> = ({
  icon: Icon,
  title,
  body,
  onAdd,
}) => {
  const styles = useStyles();
  return (
    <div className={styles.root}>
      <Icon className={styles.icon} />
      <Subtitle1>{title}</Subtitle1>
      <Body1 className={styles.body}>{body}</Body1>
      {onAdd && (
        <Button className={styles.action} appearance="primary" icon={<Add20Regular />} onClick={onAdd}>
          Add download
        </Button>
      )}
    </div>
  );
};
