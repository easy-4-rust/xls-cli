'use strict';

const { existsSync } = require('fs');
const { dirname, join } = require('path');

function linuxLibc() {
  if (process.platform !== 'linux') return null;
  const report = process.report && process.report.getReport
    ? process.report.getReport()
    : null;
  return report && report.header && report.header.glibcVersionRuntime ? 'gnu' : 'musl';
}

function packageName() {
  const arch = process.arch;
  if (!['x64', 'arm64'].includes(arch)) {
    return `unsupported-${process.platform}-${arch}`;
  }
  if (process.platform === 'darwin') return `@easy4rust/xls-cli-darwin-${arch}`;
  if (process.platform === 'win32') return `@easy4rust/xls-cli-win32-${arch}`;
  if (process.platform === 'linux') {
    return `@easy4rust/xls-cli-linux-${arch}-${linuxLibc()}`;
  }
  return `unsupported-${process.platform}-${arch}`;
}

function resolveBinary() {
  if (process.env.XLS_CLI_BINARY) return process.env.XLS_CLI_BINARY;
  const packageId = packageName();
  try {
    const packageJson = require.resolve(`${packageId}/package.json`);
    const executable = process.platform === 'win32' ? 'xls.exe' : 'xls';
    return join(dirname(packageJson), 'bin', executable);
  } catch (_) {
    // Source checkout fallback only; never downloads or compiles at npm runtime.
    const executable = process.platform === 'win32' ? 'xls.exe' : 'xls';
    const release = join(__dirname, '..', 'target', 'release', executable);
    if (existsSync(release)) return release;
    const debug = join(__dirname, '..', 'target', 'debug', executable);
    return existsSync(debug) ? debug : null;
  }
}

module.exports = { linuxLibc, packageName, resolveBinary };
