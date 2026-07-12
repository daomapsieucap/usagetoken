/**
 * Stages the ccusage native binary as a Tauri sidecar.
 *
 * ccusage's `cli.js` is just a thin dispatcher that spawns a real,
 * platform-specific native binary shipped as an optional dependency
 * (e.g. `@ccusage/ccusage-win32-x64`). We copy that native binary
 * directly instead of bundling cli.js, since cli.js's isMainModule()
 * check never passes inside a compiled standalone executable, which
 * silently turns the whole CLI into a no-op.
 *
 * Run with: pnpm build:sidecar
 * Output: src-tauri/binaries/ccusage-{triple}[.exe]
 */

import { copyFileSync, chmodSync, mkdirSync, realpathSync } from 'fs';
import { createRequire } from 'module';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, '..');
// Resolve from within ccusage's own package (following pnpm's
// symlink into the virtual store) so its optional native-binary
// dependencies, which aren't hoisted to the project root, are visible.
const ccusagePkgPath = realpathSync(path.join(root, 'node_modules', 'ccusage', 'package.json'));
const require = createRequire(ccusagePkgPath);

// Derive Rust target triple and ccusage's native package name/binary path
// from Node's process.platform/arch.
const platformArchMap = {
    'win32-x64':    ['x86_64-pc-windows-msvc',   '@ccusage/ccusage-win32-x64',   'bin/ccusage.exe'],
    'win32-arm64':  ['aarch64-pc-windows-msvc',  '@ccusage/ccusage-win32-arm64', 'bin/ccusage.exe'],
    'darwin-x64':   ['x86_64-apple-darwin',      '@ccusage/ccusage-darwin-x64',  'bin/ccusage'],
    'darwin-arm64': ['aarch64-apple-darwin',     '@ccusage/ccusage-darwin-arm64','bin/ccusage'],
    'linux-x64':    ['x86_64-unknown-linux-gnu', '@ccusage/ccusage-linux-x64',   'bin/ccusage'],
    'linux-arm64':  ['aarch64-unknown-linux-gnu','@ccusage/ccusage-linux-arm64', 'bin/ccusage'],
};
const key = `${process.platform}-${process.arch}`;
const mapping = platformArchMap[key];
if (!mapping) throw new Error(`Unsupported platform/arch: ${key}`);
const [triple, nativePackage, binSubpath] = mapping;

// Resolve the native binary from the ccusage optional dependency package.
const pkgJsonPath = require.resolve(path.posix.join(nativePackage, 'package.json'));
const entryPath = path.join(path.dirname(pkgJsonPath), ...binSubpath.split('/'));

// Output path (Tauri sidecar naming convention: {name}-{triple}[.exe])
const outDir = path.join(root, 'src-tauri', 'binaries');
mkdirSync(outDir, { recursive: true });
const isWindows = triple.includes('windows');
const outFile = path.join(outDir, `ccusage-${triple}${isWindows ? '.exe' : ''}`);

console.log(`Triple  : ${triple}`);
console.log(`Source  : ${entryPath}`);
console.log(`Output  : ${outFile}`);
console.log('');

copyFileSync(entryPath, outFile);
if (!isWindows) chmodSync(outFile, 0o755);

console.log('Sidecar staged successfully.');
