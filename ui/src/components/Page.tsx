import React from 'react';
import { Caption1, Text, makeStyles, tokens } from '@fluentui/react-components';

const useStyles = makeStyles({
  scroller: { flex: 1, overflowY: 'auto' },
  inner: {
    maxWidth: '1000px',
    padding: '28px 36px 36px',
    display: 'flex',
    flexDirection: 'column',
    gap: '4px',
  },
  titleRow: { display: 'flex', alignItems: 'center', gap: '12px', marginBottom: '20px' },
  title: {
    flex: 1,
    fontSize: '28px',
    lineHeight: '36px',
    fontWeight: tokens.fontWeightSemibold,
  },
  section: {
    fontWeight: tokens.fontWeightSemibold,
    marginTop: '20px',
    marginBottom: '6px',
  },
  card: {
    display: 'flex',
    alignItems: 'center',
    gap: '16px',
    minHeight: '68px',
    padding: '12px 16px',
    boxSizing: 'border-box',
    backgroundColor: 'var(--cfx-card)',
    border: '1px solid var(--cfx-stroke)',
    borderRadius: tokens.borderRadiusLarge,
  },
  cardIcon: { fontSize: '20px', display: 'flex', color: tokens.colorNeutralForeground1, flexShrink: 0 },
  cardText: { flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column' },
  muted: { color: tokens.colorNeutralForeground3 },
  control: { flexShrink: 0, display: 'flex', alignItems: 'center', gap: '8px' },
});

export const Page: React.FC<{ title: string; actions?: React.ReactNode; children: React.ReactNode }> = ({
  title,
  actions,
  children,
}) => {
  const styles = useStyles();
  return (
    <div className={styles.scroller}>
      <div className={styles.inner}>
        <div className={styles.titleRow}>
          <h1 className={styles.title} style={{ margin: 0 }}>
            {title}
          </h1>
          {actions}
        </div>
        {children}
      </div>
    </div>
  );
};

export const SectionHeader: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const styles = useStyles();
  return (
    <Text as="h2" className={styles.section} block>
      {children}
    </Text>
  );
};

/** Windows 11 Settings-style row: icon, title + description, control on the right. */
export const SettingsCard: React.FC<{
  icon?: React.ReactNode;
  title: React.ReactNode;
  description?: React.ReactNode;
  children?: React.ReactNode;
}> = ({ icon, title, description, children }) => {
  const styles = useStyles();
  return (
    <div className={styles.card}>
      {icon && <span className={styles.cardIcon}>{icon}</span>}
      <div className={styles.cardText}>
        <Text>{title}</Text>
        {description && <Caption1 className={styles.muted}>{description}</Caption1>}
      </div>
      {children && <div className={styles.control}>{children}</div>}
    </div>
  );
};
