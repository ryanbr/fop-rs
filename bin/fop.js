#!/usr/bin/env node

const { spawn } = require('child_process');
const path = require('path');
const fs = require('fs');

const isWindows = process.platform === 'win32';
const binaryPath = path.join(__dirname, isWindows ? 'fop-binary.exe' : 'fop-binary');

// Check if binary exists
if (!fs.existsSync(binaryPath)) {
  console.error('Error: FOP binary not found.');
  console.error('');
  console.error('The binary may not have been downloaded during installation.');
  console.error('Try reinstalling: npm install -g fop-cli');
  console.error('');
  console.error('Or run the install script manually:');
  console.error(`  node ${path.join(__dirname, '..', 'install.js')}`);
  process.exit(1);
}

// Spawn the binary with all arguments
const child = spawn(binaryPath, process.argv.slice(2), {
  stdio: 'inherit',
  env: process.env
});

child.on('error', (err) => {
  console.error('Failed to start FOP:', err.message);
  process.exit(1);
});

child.on('close', (code) => {
  process.exit(code || 0);
});
