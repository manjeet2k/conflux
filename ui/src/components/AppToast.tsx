import React from 'react';
import { Button, Toast, ToastBody, ToastTitle, ToastTrigger } from '@fluentui/react-components';
import type { ToastProps } from '@fluentui/react-components';
import { Dismiss16Regular } from '@fluentui/react-icons';

interface AppToastProps extends Pick<ToastProps, 'onClick' | 'style'> {
  title: string;
  subtitle?: string;
  children?: React.ReactNode;
  onView?: () => void;
}

export const AppToast: React.FC<AppToastProps> = ({ title, subtitle, children, onView, onClick, style }) => {
  const handleClick = (event: React.MouseEvent<HTMLDivElement>) => {
    onClick?.(event);
    if (event.defaultPrevented) return;
    if (window.getSelection()?.toString().trim()) event.preventDefault();
  };
  const dismiss = (
    <ToastTrigger>
      <Button appearance="subtle" aria-label="Dismiss notification" icon={<Dismiss16Regular />} size="small" />
    </ToastTrigger>
  );
  const actions = onView ? (
    <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
      <ToastTrigger>
        <Button appearance="transparent" size="small" onClick={onView}>View</Button>
      </ToastTrigger>
      {dismiss}
    </div>
  ) : dismiss;

  return (
    <ToastTrigger disableButtonEnhancement>
      <Toast onClick={handleClick} style={{ cursor: 'pointer', ...style }}>
        <ToastTitle action={actions}>{title}</ToastTitle>
        {children && <ToastBody subtitle={subtitle}>{children}</ToastBody>}
      </Toast>
    </ToastTrigger>
  );
};
