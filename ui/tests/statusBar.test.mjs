import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';

test('StatusBar renders bare tortoise toggle without dropdowns or labels', () => {
  const tsx = fs.readFileSync(new URL('../src/components/StatusBar.tsx', import.meta.url), 'utf8');
  const css = fs.readFileSync(new URL('../src/components/StatusBar.css', import.meta.url), 'utf8');

  // Verify StatusBar renders bare tortoise button
  assert.match(
    tsx,
    /className=\{`cfx-slow-mode-tortoise/,
    'StatusBar.tsx must render cfx-slow-mode-tortoise'
  );

  // Must not contain SplitButton or Menu dropdown in StatusBar.tsx
  assert.doesNotMatch(tsx, /SplitButton/, 'StatusBar.tsx must not use SplitButton');
  assert.doesNotMatch(tsx, /MenuTrigger/, 'StatusBar.tsx must not use MenuTrigger');
  assert.doesNotMatch(tsx, /MenuItemRadio/, 'StatusBar.tsx must not use MenuItemRadio');

  // Must not have text label inside the slow mode button (just tortoise icon)
  assert.doesNotMatch(tsx, />\s*Slow mode\s*<\/button>/, 'StatusBar.tsx must not render text label inside tortoise toggle');

  // CSS must provide bare tortoise button styling with no borders or background
  assert.match(
    css,
    /body\s+\.cfx-slow-mode-tortoise/,
    'StatusBar.css must contain .cfx-slow-mode-tortoise selector'
  );
  assert.match(
    css,
    /background-color:\s*transparent/,
    'StatusBar.css must set transparent background for bare tortoise'
  );
  assert.match(
    css,
    /border:\s*0/,
    'StatusBar.css must have zero border for bare tortoise'
  );
  assert.match(
    css,
    /\.cfx-slow-mode-tortoise:hover\s*\{\s*transform:\s*none;\s*\}/,
    'StatusBar.css must not increase size on hover'
  );
  assert.match(
    css,
    /\.cfx-slow-mode-tortoise:active\s*\{\s*transform:\s*scale\(/,
    'StatusBar.css must scale down on press'
  );
  assert.match(
    css,
    /@keyframes\s+cfxSlowModeSpring/,
    'StatusBar.css must define cfxSlowModeSpring animation'
  );
  assert.match(
    css,
    /\.cfx-slow-mode-spring/,
    'StatusBar.css must define .cfx-slow-mode-spring class'
  );
  assert.match(
    tsx,
    /cfx-slow-mode-spring/,
    'StatusBar.tsx must apply spring class on tap'
  );
});

test('SettingsPage provides slow mode custom input field in MB/s with not-filled tortoise icon', () => {
  const tsx = fs.readFileSync(new URL('../src/features/settings/components/SettingsView.tsx', import.meta.url), 'utf8');

  // Verify SettingsCard with not-filled TurtleIcon
  assert.match(
    tsx,
    /<TurtleIcon\s+active=\{false\}\s+width=\{24\}\s+height=\{20\}\s*\/>/,
    'SettingsPage.tsx must use not-filled TurtleIcon (active={false})'
  );

  // Must provide custom input field always in MB/s
  assert.match(
    tsx,
    /contentAfter="MB\/s"/,
    'SettingsPage.tsx must display MB/s in custom input field'
  );
  assert.match(
    tsx,
    /title="Slow mode speed limit"/,
    'SettingsPage.tsx must contain Slow mode speed limit setting card'
  );
});
