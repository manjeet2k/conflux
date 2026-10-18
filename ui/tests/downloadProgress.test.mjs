import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';

test('DownloadProgress component defines vibrant pill hue-shift gradients and variants', () => {
  const content = fs.readFileSync(new URL('../src/components/DownloadProgress.tsx', import.meta.url), 'utf8');

  // Verify Option 1 Vibrant Pill Gradients (clean hue-shifts, reversed)
  assert.match(
    content,
    /'linear-gradient\(90deg, #22D3EE 0%, #3B82F6 100%\)'/,
    'Active/downloading must use Electric Cyan to Deep Sky reversed gradient'
  );
  assert.match(
    content,
    /'linear-gradient\(90deg, #A3E635 0%, #10B981 100%\)'/,
    'Completed must use Lime to Emerald reversed gradient'
  );
  assert.match(
    content,
    /'linear-gradient\(90deg, #FDE047 0%, #F59E0B 100%\)'/,
    'Paused/stopping must use Gold to Warm Amber reversed gradient'
  );
  assert.match(
    content,
    /'linear-gradient\(90deg, #FB923C 0%, #E11D48 100%\)'/,
    'Failed/error must use Tangerine to Crimson reversed gradient'
  );

  // Verify Option 2 Multi-stop Mesh / Liquid Gradients (reversed)
  assert.match(
    content,
    /'linear-gradient\(90deg, #F59E0B 0%, #EC4899 50%, #6366F1 100%\)'/,
    'Electric Aurora gradient must be present for active multi-stop transfers (reversed)'
  );
  assert.match(
    content,
    /'linear-gradient\(90deg, #FEF08A 0%, #FACC15 45%, #F97316 100%\)'/,
    'Neon Citrus gradient must be present for paused multi-stop transfers (reversed)'
  );

  // Verify Option 3 Muted / Earthy Pastel Gradients (reversed)
  assert.match(
    content,
    /'linear-gradient\(90deg, #43978D 0%, #264D59 100%\)'/,
    'Seafoam to Sage reversed gradient must be present for pastel transfers'
  );
  assert.match(
    content,
    /'linear-gradient\(90deg, #F9E07F 0%, #43978D 100%\)'/,
    'Sage to Gold reversed gradient must be present for pastel completed transfers'
  );
  assert.match(
    content,
    /'linear-gradient\(90deg, #F9E07F 0%, #F9AD6A 100%\)'/,
    'Warm Amber to Peach reversed gradient must be present for pastel paused transfers'
  );
  assert.match(
    content,
    /'linear-gradient\(90deg, #F9AD6A 0%, #D46C4E 100%\)'/,
    'Terracotta to Coral reversed gradient must be present for pastel error transfers'
  );

  // Must not fade from transparent in the fill background
  assert.doesNotMatch(
    content,
    /color-mix\(in srgb, var\(--cfx-bar\) 40%, transparent\)/,
    'DownloadProgress must not fade single color from transparent to solid'
  );

  // Must not have external glow or drop-shadow
  assert.doesNotMatch(
    content,
    /boxShadow/,
    'DownloadProgress must not have boxShadow glow effect'
  );
  assert.doesNotMatch(
    content,
    /--cfx-bar-glow/,
    'DownloadProgress must not define --cfx-bar-glow properties'
  );
  assert.match(
    content,
    /track:\s*\{[^}]*overflow:\s*'hidden'/,
    'DownloadProgress track must be cleanly clipped with overflow: hidden'
  );
});
