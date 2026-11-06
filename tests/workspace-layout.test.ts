import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const read = (path: string) => readFileSync(new URL('../' + path, import.meta.url), 'utf8');

test('shared shell owns the viewport; hidden retained panes are excluded regardless of feature display', () => {
  const css = read('src/styles.css');
  const shell = css.match(/\.studio-shell\s*\{([^}]+)\}/)?.[1] ?? '';
  assert.match(shell, /height:\s*100dvh/);
  assert.match(shell, /min-height:\s*0/);
  assert.match(shell, /overflow:\s*hidden/);
  assert.match(css, /\[hidden\]:not\(\[hidden="until-found"\]\)\s*\{\s*display:\s*none\s*!important/);
  const setup = read('src/components/setup/setup.css');
  assert.match(setup, /\.aio-studio-host\s*\{[^}]*flex:\s*1[^}]*min-height:\s*0/);
  assert.match(setup, /\.aio-decision-area\s*\{[^}]*align-items:\s*stretch/);
  assert.match(setup, /\.aio-setup\s*>\s*\.footer\s*\{[^}]*position:\s*fixed/);
});

test('tabs consume parent space instead of reintroducing independent viewport height formulas', () => {
  for (const path of ['src/features/forge/forge.css', 'src/features/chat/chat.css']) {
    assert.doesNotMatch(read(path), /height:\s*calc\(100d?vh\s*-/);
  }
  const workbench = read('src/features/studio/Workbench.tsx');
  assert.doesNotMatch(workbench, /setFit\(|height:\s*fit\b/);
  assert.match(workbench, /className=\{`workbench-host\$\{beginner\?' beginner-mode':''\}`\} hidden=\{planVisible\|\|\(beginner&&beginnerPage!=='generate'\)\}/);
  assert.match(read('src/components/setup/AiOSetup.tsx'), /className="aio-studio-host" hidden=\{view!==\'studio\'\}/);
});

test('CI blocks layout regressions using actual dynamically discovered tabs and negative controls', () => {
  assert.match(read('.github/workflows/verify.yml'), /npm run test:layout/);
  const script = read('tools/verify_workspace_layout.py');
  assert.match(script, /\.aio-tabs \[role="tab"\]/);
  assert.match(script, /negative control: footer-only gap is rejected/);
  assert.match(script, /long future content reachable/);
});
