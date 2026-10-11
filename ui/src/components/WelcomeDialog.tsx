import React, { useId, useRef, useState } from 'react';
import {
  Button, Dialog, DialogSurface, DialogTitle,
} from '@fluentui/react-components';
import {
  ArrowDownload24Regular, ArrowLeft20Regular, ArrowRight20Regular,
  Checkmark20Regular,
  Link24Regular, Pause24Regular, Play24Regular, PlugConnected24Regular,
} from '@fluentui/react-icons';
import { api, errorText } from '../api';
import type { AboutLink } from '../types';
import chromeLogo from '../assets/browsers/chrome.svg';
import firefoxLogo from '../assets/browsers/firefox.svg';
import './WelcomeDialog.css';

/** Decorative diagrams use theme colors so they work in light and dark mode. */
const SetupIllustration: React.FC<{ step: number }> = ({ step }) => {
  const gradientId = useId();
  return (
  <svg viewBox="0 0 400 300" className="cfx-setup-illustration" aria-hidden="true" focusable="false">
    <defs>
      <linearGradient id={gradientId} x1="0" y1="0" x2="1" y2="1">
        <stop stopColor="#497bff" /><stop offset="1" stopColor="#9b52e8" />
      </linearGradient>
    </defs>
    <circle cx="200" cy="150" r="130" fill="currentColor" opacity=".06" />
    <circle cx="200" cy="150" r="100" fill="none" stroke="currentColor" opacity=".12" strokeDasharray="4 8" />
    {step === 0 && <>
      <g fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round">
        <path opacity=".2" d="M100 68C140 68 126 150 166 150M100 150H166M100 232C140 232 126 150 166 150" />
      </g>
      {[38, 120, 202].map((y, index) => <g key={y} className={`cfx-setup-source cfx-setup-source-${index}`}>
        <rect x="38" y={y} width="62" height="60" rx="16" className="cfx-setup-card" />
        <g transform={`translate(0, ${(index - 1) * 20})`}>
        {index === 0 ? <g fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round">
          <path d="M51 81Q69 63 87 81M57 88Q69 76 81 88" /><circle cx="69" cy="95" r="2" fill="currentColor" />
        </g> : index === 1 ? <g fill="none" stroke="currentColor" strokeWidth="3">
          <rect x="57" y="135" width="24" height="20" rx="3" /><path d="M63 155V163M69 155V163M75 155V163" />
        </g> : <g fill="none" stroke="currentColor" strokeWidth="3">
          <rect x="59" y="193" width="20" height="36" rx="4" /><path d="M66 222H72" />
        </g>}
        </g>
      </g>)}
      {['M100 68C140 68 126 150 166 150', 'M100 150H166', 'M100 232C140 232 126 150 166 150'].map((d, index) => <path key={d} d={d} pathLength="100" fill="none" stroke="currentColor" strokeWidth="4" strokeLinecap="round" className={`cfx-setup-packet cfx-setup-packet-${index}`} />)}
      <rect x="166" y="101" width="100" height="98" rx="24" fill={`url(#${gradientId})`} />
      <g stroke="#fff" strokeWidth="5" strokeLinecap="round" strokeLinejoin="round" fill="none">
        <path d="M216 123V158M201 147L216 162L231 147M195 178H237" />
      </g>
      <path className="cfx-setup-path" d="M274 150H302" stroke="currentColor" strokeWidth="3" />
      <rect x="305" y="122" width="54" height="58" rx="10" className="cfx-setup-card" />
      <path d="M318 143H345M318 153H345M318 163H336" stroke="currentColor" strokeWidth="3" strokeLinecap="round" />
      <circle cx="264" cy="104" r="8" fill="currentColor" opacity=".25" />
    </>}
    {step === 1 && <>
      {/* A download link is clicked, carried to Conflux, then added to its queue. */}
      <rect x="22" y="30" width="226" height="148" rx="16" className="cfx-setup-card" />
      <path d="M22 64H248" stroke="currentColor" opacity=".18" />
      {[40, 53, 66].map((x, index) => <circle key={x} cx={x} cy="47" r="3.5" fill={['#f29b63', '#c49bf3', '#5bbdad'][index]} />)}
      <rect x="85" y="41" width="90" height="12" rx="6" fill="currentColor" opacity=".1" />
      <image href={chromeLogo} x="186" y="37" width="20" height="20" />
      <image href={firefoxLogo} x="215" y="37" width="20" height="20" />
      <rect x="42" y="82" width="128" height="8" rx="4" fill="currentColor" opacity=".16" />
      <rect x="42" y="98" width="86" height="6" rx="3" fill="currentColor" opacity=".09" />
      <rect x="42" y="119" width="119" height="36" rx="9" fill="#577af1" opacity=".12" />
      <rect className="cfx-handoff-click" x="42" y="119" width="119" height="36" rx="9" fill="#577af1" opacity="0" />
      <path d="M62 128V143M56 138L62 144L68 138M81 136H143" stroke="#638cf5" strokeWidth="3" fill="none" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M160 150C195 150 180 226 226 226" stroke="currentColor" strokeWidth="2" strokeDasharray="4 6" fill="none" opacity=".3" />
      <rect x="201" y="166" width="177" height="108" rx="16" className="cfx-setup-card" />
      <rect x="215" y="180" width="29" height="29" rx="9" fill={`url(#${gradientId})`} />
      <path d="M229.5 186V199M224 194L229.5 200L235 194M223 204H236" fill="none" stroke="#fff" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
      <text x="253" y="200" fill="var(--colorNeutralForeground1)" fontSize="14" fontWeight="600">Conflux</text>
      <rect x="216" y="219" width="146" height="39" rx="9" fill="currentColor" opacity=".06" />
      <g className="cfx-handoff-arrival">
        <rect x="225" y="227" width="17" height="23" rx="4" fill="#6888ed" opacity=".25" />
        <path d="M230 234H237M230 239H237" stroke="#638cf5" strokeWidth="2" strokeLinecap="round" />
        <rect x="250" y="229" width="67" height="5" rx="2.5" fill="currentColor" opacity=".4" />
        <rect x="250" y="240" width="85" height="4" rx="2" fill="currentColor" opacity=".1" />
        <rect className="cfx-handoff-progress" x="250" y="240" width="72" height="4" rx="2" fill="#28ac96" />
      </g>
      <g className="cfx-handoff-file">
        <rect x="91" y="121" width="22" height="29" rx="5" fill="#638cf5" />
        <path d="M97 130H107M97 136H107M97 142H104" stroke="#fff" strokeWidth="2" strokeLinecap="round" />
      </g>
      <g className="cfx-handoff-cursor" fill="var(--colorNeutralBackground1)" stroke="#638cf5" strokeWidth="2" strokeLinejoin="round">
        <path d="M131 138V162L137 156L143 168L149 165L143 153H153Z" />
      </g>
    </>}
    {step === 2 && <>
      <rect x="35" y="56" width="330" height="192" rx="18" className="cfx-setup-card" />
      <path d="M35 92H365" stroke="currentColor" opacity=".2" />
      <rect x="52" y="69" width="88" height="10" rx="5" fill="currentColor" opacity=".25" />
      <rect x="53" y="112" width="45" height="54" rx="10" fill="currentColor" opacity=".12" />
      <path d="M68 127H83M68 135H83M68 143H79" stroke="currentColor" strokeWidth="3" strokeLinecap="round" />
      <rect x="115" y="117" width="160" height="10" rx="5" fill="currentColor" opacity=".25" />
      <rect x="115" y="138" width="207" height="8" rx="4" fill="currentColor" opacity=".1" />
      <rect className="cfx-setup-progress" x="115" y="138" width="145" height="8" rx="4" fill="currentColor" />
      <path d="M55 223H270" stroke="currentColor" opacity=".15" />
      <path className="cfx-setup-chart" pathLength="1" d="M58 213L85 203L107 210L131 185L155 193L178 178L204 183L230 161L256 167" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round" />
      <circle cx="322" cy="213" r="33" fill="#16866d" />
      <path className="cfx-setup-check" pathLength="1" d="M307 213L318 224L339 201" fill="none" stroke="#fff" strokeWidth="5" strokeLinecap="round" strokeLinejoin="round" />
    </>}
  </svg>
  );
};

interface WelcomeDialogProps {
  open: boolean;
  onFinish: (addDownload: boolean) => void;
  onError: (message: string) => void;
}

const steps = [
  { label: 'Welcome', title: 'A little setup.\nA lot more possibility.', description: 'Welcome to Conflux. Bring your connections together and make every download feel effortless.', caption: 'More connections. One download.' },
  { label: 'Browser', title: 'Your browser.\nMeet your download manager.', description: 'Add the companion extension to send downloads straight from your browser to Conflux.', caption: 'From a click to your download queue.' },
  { label: 'Ready', title: 'All set for\nyour next big download.', description: 'Copy a download link, choose a folder, and let Conflux take it from there.', caption: 'Every download, beautifully in view.' },
];

export const WelcomeDialog: React.FC<WelcomeDialogProps> = ({ open, onFinish, onError }) => {
  const [step, setStep] = useState(0);
  const [backwards, setBackwards] = useState(false);
  const [animationPaused, setAnimationPaused] = useState(false);
  const titleRef = useRef<HTMLHeadingElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const current = steps[step];
  const goTo = (next: number) => {
    setBackwards(next < step);
    setStep(next);
    scrollRef.current?.scrollTo({ top: 0 });
    // Keep keyboard and screen-reader context on the new step, not a replaced button.
    titleRef.current?.focus({ preventScroll: true });
  };
  const openBrowser = (target: AboutLink) => {
    api.openAboutLink(target).catch((e) => onError(`Could not open browser extension page: ${errorText(e)}`));
  };

  return (
    <Dialog open={open} onOpenChange={(_, data) => !data.open && onFinish(false)}>
      <DialogSurface className="cfx-welcome">
        <header className="cfx-welcome-header">
          <span className="cfx-welcome-brand"><span className="cfx-welcome-logo"><ArrowDownload24Regular /></span> Conflux</span>
          <span className="cfx-welcome-count">Step {step + 1} of {steps.length}</span>
        </header>
        <div className="cfx-welcome-scroll" ref={scrollRef}>
          <div className={`cfx-welcome-layout cfx-welcome-step-${step}`}>
            <div className={`cfx-welcome-visual${animationPaused ? ' cfx-animation-paused' : ''}`} key={`art-${step}`}>
              <span className="cfx-welcome-orb cfx-welcome-orb-one" />
              <span className="cfx-welcome-orb cfx-welcome-orb-two" />
              <span className="cfx-welcome-visual-label">{step === 0 ? 'BETTER TOGETHER' : step === 1 ? 'A NATURAL CONNECTION' : 'READY WHEN YOU ARE'}</span>
              <SetupIllustration step={step} />
              <p className="cfx-welcome-caption">{current.caption}</p>
              <Button appearance="subtle" size="small" icon={animationPaused ? <Play24Regular /> : <Pause24Regular />} onClick={() => setAnimationPaused((value) => !value)} className="cfx-welcome-animation-control">{animationPaused ? 'Play animation' : 'Pause animation'}</Button>
            </div>
            <section className="cfx-welcome-copy">
              <span className="cfx-welcome-eyebrow">{step === 0 ? 'MAKE YOURSELF AT HOME' : step === 1 ? 'OPTIONAL · ADD IT ANYTIME' : 'YOU’RE IN CONTROL'}</span>
              <DialogTitle ref={titleRef} tabIndex={-1} className="cfx-welcome-title" aria-live="polite">{current.title}</DialogTitle>
              <div key={`copy-${step}`} className={`cfx-welcome-details${backwards ? ' cfx-welcome-details-back' : ''}`}>
                <p className="cfx-welcome-description">{current.description}</p>
                {step === 0 && <ul className="cfx-welcome-features">
                  <li><span className="cfx-welcome-icon cfx-welcome-icon-blue"><PlugConnected24Regular /></span><span><strong>Put your connections to work</strong><small>Use Wi-Fi, Ethernet, or your phone’s hotspot.</small></span></li>
                  <li><span className="cfx-welcome-icon cfx-welcome-icon-purple"><Pause24Regular /></span><span><strong>Pick up where you left off</strong><small>Pause your downloads and resume when you’re ready.</small></span></li>
                  <li><span className="cfx-welcome-icon cfx-welcome-icon-green"><ArrowDownload24Regular /></span><span><strong>Keep everything in view</strong><small>Your files, progress, and speeds in one place.</small></span></li>
                </ul>}
                {step === 1 && <>
                  <div className="cfx-welcome-browsers">
                    <Button className="cfx-welcome-browser" icon={<span className="cfx-welcome-icon cfx-welcome-icon-blue"><img src={chromeLogo} width="28" height="28" alt="" /></span>} onClick={() => openBrowser('extension-chrome')}><span><strong>Chrome</strong><small>Also for Edge & Brave</small></span><ArrowRight20Regular /></Button>
                    <Button className="cfx-welcome-browser" icon={<span className="cfx-welcome-icon cfx-welcome-icon-orange"><img src={firefoxLogo} width="28" height="28" alt="" /></span>} onClick={() => openBrowser('extension-firefox')}><span><strong>Firefox</strong><small>Connect your browser</small></span><ArrowRight20Regular /></Button>
                  </div>
                  <p className="cfx-welcome-note">Finish installing in your browser, then come back here. You can also find these links in Settings → Browser Integration.</p>
                </>}
                {step === 2 && <>
                  <div className="cfx-welcome-first-download"><span className="cfx-welcome-icon cfx-welcome-icon-purple"><Link24Regular /></span><span><strong>Have a download link?</strong><small>Choose “Add first download” below to get started.</small></span></div>
                  <p className="cfx-welcome-note">Just looking around? Head to your dashboard. You can add a download whenever you’re ready.</p>
                </>}
              </div>
            </section>
          </div>
        </div>
        <footer className="cfx-welcome-footer">
          <ol className="cfx-welcome-steps" aria-label="Setup progress">
            {steps.map(({ label }, index) => <li key={label} className={index <= step ? 'is-reached' : ''} aria-current={index === step ? 'step' : undefined}><span>{index < step ? <Checkmark20Regular /> : index + 1}</span><span className="cfx-welcome-step-label">{label}</span></li>)}
          </ol>
          <div className="cfx-welcome-actions">
            {step > 0 && <Button appearance="subtle" icon={<ArrowLeft20Regular />} onClick={() => goTo(step - 1)}>Back</Button>}
            <Button appearance="secondary" onClick={() => step === 1 ? goTo(2) : onFinish(false)}>{step === 0 ? 'Skip setup' : step === 1 ? 'Skip for now' : 'Go to dashboard'}</Button>
            <Button appearance="primary" icon={step === 2 ? <ArrowDownload24Regular /> : <ArrowRight20Regular />} iconPosition="after" onClick={() => step < 2 ? goTo(step + 1) : onFinish(true)}>{step === 0 ? 'Get started' : step === 1 ? 'Continue' : 'Add first download'}</Button>
          </div>
        </footer>
      </DialogSurface>
    </Dialog>
  );
};
