#!/usr/bin/env node

const https = require('https');
const http = require('http');
const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

// Configuration - UPDATE THESE FOR YOUR RELEASE
const VERSION = '3.9.5';
const GITHUB_REPO = 'ryanbr/fop-rs'; // Change to your repo
const BINARY_NAME = 'fop';

// Platform mapping
const PLATFORMS = {
  'darwin-x64': 'fop-macos-x86_64',
  'darwin-arm64': 'fop-macos-arm64',
  'linux-x64': 'fop-linux-x86_64',
  'linux-arm64': 'fop-linux-arm64',
};

function getPlatformBinary() {
  const platform = process.platform;
  const arch = process.arch;
  const key = `${platform}-${arch}`;
  
  const binary = PLATFORMS[key];
  if (!binary) {
    console.error(`Unsupported platform: ${platform}-${arch}`);
    console.error('Supported platforms:', Object.keys(PLATFORMS).join(', '));
    process.exit(1);
  }
  
  return binary;
}

function getDownloadUrl(binaryName) {
  // GitHub releases URL pattern
  return `https://github.com/${GITHUB_REPO}/releases/download/v${VERSION}/${binaryName}`;
}

function download(url, dest) {
  return new Promise((resolve, reject) => {
    console.log(`Downloading from: ${url}`);
    
    const makeRequest = (url) => {
      const protocol = url.startsWith('https') ? https : http;
      
      protocol.get(url, (response) => {
        // Handle redirects
        if (response.statusCode === 301 || response.statusCode === 302) {
          const redirectUrl = response.headers.location;
          console.log(`Following redirect to: ${redirectUrl}`);
          makeRequest(redirectUrl);
          return;
        }
        
        if (response.statusCode !== 200) {
          reject(new Error(`Download failed with status ${response.statusCode}`));
          return;
        }
        
        const file = fs.createWriteStream(dest);
        response.pipe(file);
        
        file.on('finish', () => {
          file.close();
          resolve();
        });
        
        file.on('error', (err) => {
          fs.unlink(dest, () => {});
          reject(err);
        });
      }).on('error', reject);
    };
    
    makeRequest(url);
  });
}

async function install() {
  const binDir = path.join(__dirname, 'bin');
  const binaryPath = path.join(binDir, 'fop-binary');
  
  // Create bin directory
  if (!fs.existsSync(binDir)) {
    fs.mkdirSync(binDir, { recursive: true });
  }
  
  // Check if already installed
  if (fs.existsSync(binaryPath)) {
    console.log('FOP binary already exists, skipping download.');
    return;
  }
  
  const platformBinary = getPlatformBinary();
  const url = getDownloadUrl(platformBinary);
  
  console.log(`Installing FOP v${VERSION} for ${process.platform}-${process.arch}...`);
  
  try {
    await download(url, binaryPath);
    
    // Make executable
    fs.chmodSync(binaryPath, 0o755);
    
    console.log('FOP installed successfully!');
    console.log(`Binary location: ${binaryPath}`);
    
    // Verify installation
    try {
      const version = execSync(`"${binaryPath}" --version`, { encoding: 'utf-8' });
      console.log(`Installed: ${version.trim()}`);
    } catch (e) {
      console.log('Note: Could not verify installation.');
    }
  } catch (err) {
    console.error('Installation failed:', err.message);
    console.error('');
    console.error('You can manually download the binary from:');
    console.error(`  ${url}`);
    console.error('');
    console.error('Or build from source:');
    console.error('  1. Install Rust: https://rustup.rs');
    console.error('  2. Clone the repo and run: cargo build --release');
    process.exit(1);
  }
}

// Run installation
install();
