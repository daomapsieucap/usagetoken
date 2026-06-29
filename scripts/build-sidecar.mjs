/**
 * Compiles the bundled ccusage sidecar binary for the current rustc host triple.
 * Run with: pnpm build:sidecar
 * Output: src-tauri/binaries/ccusage-{triple}[.exe]
 */

import { execSync } from 'child_process';
import { mkdirSync } from 'fs';
import { createRequire } from 'module';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, '..');
const require = createRequire(import.meta.url);

// Detect the rustc host triple (e.g. x86_64-pc-windows-msvc)
const rustcOut = execSync('rustc -Vv', { cwd: root }).toString();
const triple = rustcOut.split('\n').find(l => l.startsWith('host:'))?.split(':')[1]?.trim();
if (!triple) throw new Error('Could not determine rustc host triple from `rustc -Vv`');

// Map rustc triple → pkg target name
const pkgTargetMap = {
  'x86_64-pc-windows-msvc':    'node18-win-x64',
  'i686-pc-windows-msvc':      'node18-win-x86',
  'x86_64-apple-darwin':       'node18-macos-x64',
  'aarch64-apple-darwin':      'node18-macos-arm64',
  'x86_64-unknown-linux-gnu':  'node18-linux-x64',
  'aarch64-unknown-linux-gnu': 'node18-linux-arm64',
};
const pkgTarget = pkgTargetMap[triple];
if (!pkgTarget) throw new Error(`No pkg target mapping for rustc triple: ${triple}`);

// Resolve ccusage's JS entry point from its installed package.json
const ccusagePkg = require(path.join(root, 'node_modules', 'ccusage', 'package.json'));
const binField = ccusagePkg.bin;
const relEntry = typeof binField === 'string' ? binField : Object.values(binField)[0];
const entryPath = path.resolve(root, 'node_modules', 'ccusage', relEntry);

// Output path (Tauri sidecar naming convention: {name}-{triple}[.exe])
const outDir = path.join(root, 'src-tauri', 'binaries');
mkdirSync(outDir, { recursive: true });
const isWindows = triple.includes('windows');
const outFile = path.join(outDir, `ccusage-${triple}${isWindows ? '.exe' : ''}`);

console.log(`Triple  : ${triple}`);
console.log(`Entry   : ${entryPath}`);
console.log(`Output  : ${outFile}`);
console.log('');

execSync(
  `npx @yao-pkg/pkg "${entryPath}" --target ${pkgTarget} --output "${outFile}"`,
  { stdio: 'inherit', cwd: root }
);

console.log('\nSidecar built successfully.');
