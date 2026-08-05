#!/usr/bin/env node
'use strict';

const { spawnSync } = require('child_process');
const { existsSync } = require('fs');
const { resolveBinary, packageName } = require('./platform');

const binary = resolveBinary();
if (!binary || !existsSync(binary)) {
  console.error(
    `xls-cli: native binary not found for ${packageName()}. ` +
      'Reinstall with optionalDependencies enabled or build this source checkout with Cargo.'
  );
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: 'inherit' });
if (result.error) {
  console.error(`xls-cli: failed to run native binary: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status === null ? 1 : result.status);
