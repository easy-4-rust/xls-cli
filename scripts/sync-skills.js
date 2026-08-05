#!/usr/bin/env node
'use strict';

const { copyFileSync, mkdirSync } = require('fs');
const { join } = require('path');

const root = join(__dirname, '..');
const source = join(root, 'skills', 'xls-cli', 'SKILL.md');
for (const agent of ['openclaw', 'hermes']) {
  const destination = join(root, 'skills', 'dist', agent, 'xls-cli');
  mkdirSync(destination, { recursive: true });
  copyFileSync(source, join(destination, 'SKILL.md'));
}
