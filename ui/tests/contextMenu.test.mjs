import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';

test('context menu and flyout popovers have solid opaque background and no backdrop-filter', () => {
  const css = fs.readFileSync(new URL('../src/index.css', import.meta.url), 'utf8');

  // Locate the flyout popover rules
  const match = css.match(/body\s+\.fui-MenuPopover[^{]*\{([^}]+)\}/);
  assert.ok(match, 'body .fui-MenuPopover rule must be present in index.css');

  const ruleBody = match[1];

  // Must not have backdrop-filter which causes transparent hole punching in Chromium transparent windows
  assert.ok(
    !ruleBody.includes('backdrop-filter'),
    'body .fui-MenuPopover must not have backdrop-filter (causes transparency in transparent windows)'
  );

  // Must not use color-mix with transparent
  assert.ok(
    !ruleBody.includes('transparent'),
    'body .fui-MenuPopover must not have transparent color-mix'
  );

  // Must use solid theme token for background
  assert.ok(
    ruleBody.includes('var(--colorNeutralBackground1)'),
    'body .fui-MenuPopover must use var(--colorNeutralBackground1) for solid elevated fill'
  );

  // Must define border-radius: 8px
  assert.ok(
    ruleBody.includes('border-radius: 8px'),
    'body .fui-MenuPopover must have 8px border-radius matching WinUI 3 flyouts'
  );
});
