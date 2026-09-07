// Checks launch claims against executable output and compiles README Rust snippets.
// Node.js is only needed for launch maintenance, not for the Rust library/runtime.
import { readFileSync, readdirSync, existsSync, mkdtempSync, mkdirSync, rmSync } from 'node:fs';
import { resolve, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import vm from 'node:vm';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const cargo = process.env.CARGO || 'cargo';
const stdout = execFileSync(cargo, ['run', '--quiet', '--example', 'fact_history'], { cwd: root, encoding: 'utf8' });
if (!stdout.includes('PASS | Persistence, replacement, history and two-hop traversal verified')) {
  throw new Error('Fact-history assertions did not complete');
}
const html = readFileSync(join(root, 'videos/agent-memory-launch/index.html'), 'utf8');
const literal = html.match(/const SEQUENCE = (\[[\s\S]*?\n    \]);/);
if (!literal) throw new Error('Demo sequence missing');
const sequence = vm.runInNewContext(literal[1], Object.create(null), { timeout: 100 });
if (!Array.isArray(sequence) || sequence.length === 0) throw new Error('Demo sequence is empty');
for (const state of sequence) {
  if (typeof state.text !== 'string' || state.text.length === 0) throw new Error('Demo output excerpt is empty');
  if (!stdout.includes(state.text)) throw new Error(`Video excerpt differs from actual output at ${state.t}s`);
}

function markdownFiles(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
    if (['.git', 'target', 'node_modules', '.hyperframes', '.media'].includes(entry.name)) return [];
    const path = join(dir, entry.name);
    return entry.isDirectory() ? markdownFiles(path) : entry.name.endsWith('.md') ? [path] : [];
  });
}
for (const file of markdownFiles(root)) {
  const body = readFileSync(file, 'utf8').replace(/```[\s\S]*?```/g, '');
  for (const match of body.matchAll(/\]\(([^\s)]+)\)/g)) {
    const target = match[1].split('#')[0];
    if (!target || /^[a-z]+:/i.test(target)) continue;
    if (!existsSync(resolve(dirname(file), decodeURIComponent(target)))) throw new Error(`Broken link in ${file}: ${target}`);
  }
}

// rustc checks the exact Rust code blocks against the built library, without running
// file-writing README examples. Cargo examples above cover executed behavior.
const scratch = mkdtempSync(join(tmpdir(), 'agent-memory-readme-'));
try {
  const deps = join(root, 'target/debug/deps');
  execFileSync(cargo, ['build', '--lib'], { cwd: root, stdio: 'inherit' });
  const library = join(root, 'target/debug/libagent_memory.rlib');
  const rustc = process.env.RUSTC || 'rustc';
  for (const name of ['README.md', 'README.zh-CN.md']) {
    const body = readFileSync(join(root, name), 'utf8');
    let index = 0;
    for (const match of body.matchAll(/```rust\n([\s\S]*?)```/g)) {
      const out = join(scratch, `${name}-${index++}`);
      mkdirSync(out);
      execFileSync(rustc, ['--edition=2021', '--crate-name', 'readme_example', '--emit=metadata',
        '--extern', `agent_memory=${library}`, '-L', `dependency=${deps}`, '--out-dir', out, '-'],
        { cwd: root, input: match[1], stdio: ['pipe', 'inherit', 'inherit'] });
    }
    if (index === 0) throw new Error(`No Rust example found in ${name}`);
  }
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
console.log('PASS: real demo output, all video excerpts, local Markdown links and both README Rust examples');
