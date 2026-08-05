#!/usr/bin/env node
// xls-cli launcher: spawns the platform binary downloaded by install.js
// (`bin/xls` / `bin/xls.exe`) with the user's arguments and inherits stdio.
'use strict';

const { spawnSync } = require('child_process');
const { join } = require('path');
const { existsSync } = require('fs');

const exe = process.platform === 'win32' ? '.exe' : '';
const binary = join(__dirname, `xls${exe}`);

if (!existsSync(binary)) {
  console.error(
    'xls-cli: binary not found — the postinstall download may have failed.\n' +
      'Run `npm rebuild xls-cli` to retry, or place the binary manually at ' +
      binary
  );
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: 'inherit' });
if (result.error) {
  console.error(`xls-cli: failed to run ${binary}: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status === null ? 1 : result.status);
