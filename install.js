#!/usr/bin/env node

const https = require('https');
const http = require('http');
const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

// Configuration - UPDATE THESE FOR YOUR RELEASE
const VERSION = '4.2.9';
const GITHUB_REPO = 'ryanbr/fop-rs'; // Change to your repo
const BINARY_NAME = 'fop';

// Platform mapping
const PLATFORMS = {
  'darwin-x64': `-macos-x86_64`,
  'darwin-arm64': `-macos-arm64`,
  'linux-x64': `-linux-x86_64`,
  'linux-arm64': `-linux-arm64`,  // Baseline for max compatibility
  'linux-riscv64': `-linux-riscv64`,
  'win32-x64': `-windows-x86_64.exe`,
  'win32-ia32': `-windows-x86_32.exe`,
  'win32-arm64': `-windows-arm64-v2.exe`,
};

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

function buildFromSource(binaryPath) {
  console.log('Building from source...');
  console.log('This requires Rust to be installed (https://rustup.rs)');
  
  try {
    execSync('cargo --version', { stdio: 'ignore' });
  } catch (e) {
    console.error('Error: Rust/Cargo not found.');
    console.error('Please install Rust from https://rustup.rs and try again.');
    process.exit(1);
  }
  
  try {
    const tempDir = path.join(__dirname, 'build-temp');
    execSync(`git clone --depth 1 https://github.com/${GITHUB_REPO}.git "${tempDir}"`, { stdio: 'inherit' });
    execSync('cargo build --release', { cwd: tempDir, stdio: 'inherit' });
    
    const builtBinary = path.join(tempDir, 'target', 'release', process.platform === 'win32' ? 'fop.exe' : 'fop');
    fs.copyFileSync(builtBinary, binaryPath);
    fs.rmSync(tempDir, { recursive: true, force: true });
    
    console.log('Build completed successfully!');
    return true;
  } catch (e) {
    console.error('Build failed:', e.message);
    return false;
  }
}

async function install() {
  const binDir = path.join(__dirname, 'bin');
  const isWindows = process.platform === 'win32';
  const binaryPath = path.join(binDir, isWindows ? 'fop-binary.exe' : 'fop-binary');
  
  // Create bin directory
  if (!fs.existsSync(binDir)) {
    fs.mkdirSync(binDir, { recursive: true });
  }
   
  const platform = process.platform;
  const arch = process.arch;
  const key = `${platform}-${arch}`;
  const suffix = PLATFORMS[key];
  
  console.log(`Installing FOP v${VERSION} for ${platform}-${arch}...`);
  
  if (!suffix) {
    console.log(`No pre-built binary for ${platform}-${arch}, building from source...`);
    if (buildFromSource(binaryPath)) {
      fs.chmodSync(binaryPath, 0o755);
    }
    return;
  }
  
  const platformBinary = `fop-${VERSION}${suffix}`;
  const url = getDownloadUrl(platformBinary);

  
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
