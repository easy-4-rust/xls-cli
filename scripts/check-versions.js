#!/usr/bin/env node
'use strict';

const { readdirSync, readFileSync } = require('fs');
const { join } = require('path');

const expected = process.argv[2];
if (!expected) throw new Error('usage: check-versions.js VERSION');

const root = join(__dirname, '..');
const manifests = [join(root, 'package.json')];
for (const name of readdirSync(join(root, 'packages'))) {
  manifests.push(join(root, 'packages', name, 'package.json'));
}
for (const manifest of manifests) {
  const value = JSON.parse(readFileSync(manifest, 'utf8'));
  if (value.version !== expected) {
    throw new Error(`${manifest}: expected ${expected}, found ${value.version}`);
  }
}
console.log(`all npm packages use version ${expected}`);
