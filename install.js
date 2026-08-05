#!/usr/bin/env node
'use strict';

const { existsSync } = require('fs');
const { join } = require('path');
const { resolveBinary, packageName } = require('./bin/platform');

const binary = resolveBinary();
if (binary && existsSync(binary)) {
  console.log(`xls-cli: using ${binary}`);
  process.exit(0);
}

// A source checkout can build with Cargo; a published npm package intentionally
// has no compiler fallback and requires its platform optionalDependency.
if (existsSync(join(__dirname, 'Cargo.toml'))) {
  console.warn(
    `xls-cli: platform package ${packageName()} is not installed; ` +
      'run `cargo build --release` for source-checkout development.'
  );
  process.exit(0);
}

console.error(
  `xls-cli: native package ${packageName()} is unavailable. ` +
    'Ensure npm optionalDependencies are enabled and this platform is supported.'
);
process.exit(1);
