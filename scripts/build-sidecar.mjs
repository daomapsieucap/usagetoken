/**
 * Compiles the bundled ccusage sidecar binary for the current platform.
 * Requires: bun (https://bun.sh)
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

// Verify bun is available
try {
    execSync('bun --version', { stdio: 'pipe' });
} catch {
    console.error(
        'Error: bun is required to build the ccusage sidecar.\n' +
        'Install it from https://bun.sh or run: npm install -g bun\n' +
        'On Windows: winget install Oven-sh.Bun'
    );
    process.exit(1);
}

// Derive Rust target triple and bun target from Node's process.platform/arch.
const platformArchMap = {
    'win32-x64':    ['x86_64-pc-windows-msvc',    'bun-windows-x64'],
    'win32-ia32':   ['i686-pc-windows-msvc',       'bun-windows-x86'],
    'darwin-x64':   ['x86_64-apple-darwin',        'bun-darwin-x64'],
    'darwin-arm64': ['aarch64-apple-darwin',       'bun-darwin-arm64'],
    'linux-x64':    ['x86_64-unknown-linux-gnu',   'bun-linux-x64'],
    'linux-arm64':  ['aarch64-unknown-linux-gnu',  'bun-linux-arm64'],
};
const key = `${process.platform}-${process.arch}`;
const mapping = platformArchMap[key];
if (!mapping) throw new Error(`Unsupported platform/arch: ${key}`);
const [triple, bunTarget] = mapping;

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
    `bun build --compile --target=${bunTarget} "${entryPath}" --outfile "${outFile}"`,
    { stdio: 'inherit', cwd: root }
);

console.log('\nSidecar built successfully.');
