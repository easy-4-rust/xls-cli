#!/usr/bin/env node
// xls-cli installer: downloads the platform-specific xls binary from the
// easy-4-rust/xls GitHub release (esbuild-style distribution, no compile).
//
// Release asset naming (built by the xls fork's release workflow):
//   xls-<version>-<rust-target-triple>[.exe]
// e.g. xls-0.1.0-aarch64-apple-darwin, xls-0.1.0-x86_64-pc-windows-msvc.exe
//
// GitHub release downloads 302-redirect to objects.githubusercontent.com.
// Node's `https.get` does NOT follow redirects (that is undici `fetch`
// behavior), so we follow the Location header manually — up to 5 hops.
'use strict';

const { createWriteStream, chmodSync, mkdirSync, rmSync } = require('fs');
const { get } = require('https');
const { URL } = require('url');
const { join, dirname } = require('path');
const { pipeline } = require('stream');
const { promisify } = require('util');
const { platform, arch } = require('os');

const VERSION = require('./package.json').version;
const REPO = 'easy-4-rust/xls';

// platform/arch -> rust target triple
const TRIPLES = {
  'darwin-arm64': 'aarch64-apple-darwin',
  'darwin-x64': 'x86_64-apple-darwin',
  'linux-arm64': 'aarch64-unknown-linux-gnu',
  'linux-x64': 'x86_64-unknown-linux-gnu',
  'win32-arm64': 'aarch64-pc-windows-msvc',
  'win32-x64': 'x86_64-pc-windows-msvc',
};

const key = `${platform()}-${arch()}`;
const triple = TRIPLES[key];
if (!triple) {
  console.error(
    `xls-cli: unsupported platform "${key}". Supported: ${Object.keys(TRIPLES).join(', ')}`
  );
  process.exit(1);
}

const exe = platform() === 'win32' ? '.exe' : '';
const url = `https://github.com/${REPO}/releases/download/v${VERSION}/xls-${VERSION}-${triple}${exe}`;
const dest = join(__dirname, 'bin', `xls${exe}`);

// Follow redirects (Node's https does not do this automatically) and resolve
// with the final readable response; rejects on 4xx/5xx or too many hops.
function download(uri, redirectsLeft) {
  return new Promise((resolve, reject) => {
    get(uri, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        res.resume();
        if (redirectsLeft <= 0) {
          reject(new Error('too many redirects'));
          return;
        }
        const next = new URL(res.headers.location, uri).href;
        console.log(`xls-cli: redirecting to ${next}`);
        resolve(download(next, redirectsLeft - 1));
        return;
      }
      if (res.statusCode >= 400) {
        res.resume();
        reject(
          new Error(
            `HTTP ${res.statusCode} (is release v${VERSION} published at ${REPO}?)`
          )
        );
        return;
      }
      resolve(res);
    }).on('error', reject);
  });
}

mkdirSync(dirname(dest), { recursive: true });

console.log(`xls-cli: downloading ${url}`);
download(url, 5)
  .then((res) => promisify(pipeline)(res, createWriteStream(dest)))
  .then(() => {
    if (platform() !== 'win32') {
      chmodSync(dest, 0o755);
    }
    console.log(`xls-cli: installed ${dest}`);
  })
  .catch((err) => {
    // Remove a partially-written binary so a failed install never leaves a
    // broken `xls` behind.
    try {
      rmSync(dest, { force: true });
    } catch {
      /* ignore */
    }
    console.error(`xls-cli: install failed: ${err.message}`);
    process.exit(1);
  });
