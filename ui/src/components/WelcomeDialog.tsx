import React, { useState } from 'react';
import {
  Button, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface,
  DialogTitle, Text, makeStyles, tokens,
} from '@fluentui/react-components';
import { ArrowDownload48Regular } from '@fluentui/react-icons';
import { api, errorText } from '../api';
import type { AboutLink } from '../types';

const useStyles = makeStyles({
  surface: { width: '520px', maxWidth: 'calc(100vw - 48px)' },
  content: { display: 'flex', flexDirection: 'column', gap: '16px', paddingBottom: '24px' },
  icon: { color: tokens.colorBrandForeground1 },
  browsers: { display: 'flex', gap: '12px', flexWrap: 'wrap' },
  progress: { color: tokens.colorNeutralForeground3 },
});

interface WelcomeDialogProps {
  open: boolean;
  onFinish: (addDownload: boolean) => void;
  onError: (message: string) => void;
}

export const WelcomeDialog: React.FC<WelcomeDialogProps> = ({ open, onFinish, onError }) => {
  const styles = useStyles();
  const [step, setStep] = useState(0);
  const openBrowser = (target: AboutLink) => {
    api.openAboutLink(target).catch((e) => onError(`Could not open browser extension page: ${errorText(e)}`));
  };

  return (
    <Dialog open={open} onOpenChange={(_, data) => !data.open && onFinish(false)}>
      <DialogSurface className={styles.surface}>
        <DialogBody>
          <DialogTitle>{['Welcome to Conflux', 'Connect your browser', 'Ready for your first download'][step]}</DialogTitle>
          <DialogContent className={styles.content}>
            <Text size={200} className={styles.progress}>Step {step + 1} of 3</Text>
            {step === 0 && <>
              <ArrowDownload48Regular className={styles.icon} />
              <Text>Download files with your available network connections, pause when you need to, and pick up where you left off.</Text>
              <Text>You can start with a download link. Connecting your browser is optional.</Text>
            </>}
            {step === 1 && <>
              <Text>Send downloads from your browser to Conflux with the browser extension.</Text>
              <div className={styles.browsers}>
                <Button onClick={() => openBrowser('extension-chrome')}>Chrome extension</Button>
                <Button onClick={() => openBrowser('extension-firefox')}>Firefox extension</Button>
              </div>
              <Text size={200}>Open the extension page and complete installation in your browser. You can also find these links later in Settings → Browser Integration.</Text>
            </>}
            {step === 2 && <>
              <Text>Paste a download link and choose where to save your file. Conflux will show download progress and network speeds as it runs.</Text>
              <Text>You can also explore the dashboard and add a download whenever you are ready.</Text>
            </>}
          </DialogContent>
          <DialogActions>
            {step > 0 && <Button onClick={() => setStep(step - 1)}>Back</Button>}
            {step === 0 && <Button onClick={() => onFinish(false)}>Skip setup</Button>}
            {step === 1 && <Button onClick={() => setStep(2)}>Skip for now</Button>}
            {step < 2 ? <Button appearance="primary" onClick={() => setStep(step + 1)}>{step === 0 ? 'Get started' : 'Continue'}</Button> : <>
              <Button onClick={() => onFinish(false)}>Go to dashboard</Button>
              <Button appearance="primary" onClick={() => onFinish(true)}>Add first download</Button>
            </>}
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
};
