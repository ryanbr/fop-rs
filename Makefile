# FOP - Filter Orderer and Preener
# Makefile for Linux and macOS

# Detect OS and architecture
UNAME_S := $(shell uname -s)
UNAME_M := $(shell uname -m)

# Binary name
BINARY := fop

# Install location
PREFIX ?= /usr/local
BINDIR := $(PREFIX)/bin

# Rust build settings
CARGO := cargo
CARGO_FLAGS := --release

# Output directory
TARGET_DIR := target/release

# Platform-specific settings
ifeq ($(UNAME_S),Darwin)
    OS := macos
    ifeq ($(UNAME_M),arm64)
        ARCH := arm64
        TARGET := aarch64-apple-darwin
    else
        ARCH := x86_64
        TARGET := x86_64-apple-darwin
    endif
else ifeq ($(UNAME_S),Linux)
    OS := linux
    ifeq ($(UNAME_M),x86_64)
        ARCH := x86_64
        TARGET := x86_64-unknown-linux-gnu
    else ifeq ($(UNAME_M),aarch64)
        ARCH := arm64
        TARGET := aarch64-unknown-linux-gnu
    endif
else
    OS := unknown
    ARCH := unknown
endif

# Colors for output
GREEN := \033[0;32m
YELLOW := \033[0;33m
CYAN := \033[0;36m
NC := \033[0m # No Color

.PHONY: all build release debug test clean install uninstall check info help

# Default target
all: build

# Build release version
build: check-rust
	@echo "$(CYAN)Building FOP for $(OS) $(ARCH)...$(NC)"
	$(CARGO) build $(CARGO_FLAGS)
	@echo "$(GREEN)Build complete: $(TARGET_DIR)/$(BINARY)$(NC)"

# Alias for build
release: build

# Build debug version
debug: check-rust
	@echo "$(CYAN)Building FOP (debug) for $(OS) $(ARCH)...$(NC)"
	$(CARGO) build
	@echo "$(GREEN)Debug build complete: target/debug/$(BINARY)$(NC)"

# Run tests
test: check-rust
	@echo "$(CYAN)Running tests...$(NC)"
	$(CARGO) test
	@echo "$(GREEN)All tests passed!$(NC)"

# Clean build artifacts
clean:
	@echo "$(CYAN)Cleaning build artifacts...$(NC)"
	$(CARGO) clean
	@echo "$(GREEN)Clean complete$(NC)"

# Install to system
install: build
	@echo "$(CYAN)Installing $(BINARY) to $(BINDIR)...$(NC)"
	@mkdir -p $(BINDIR)
	@cp $(TARGET_DIR)/$(BINARY) $(BINDIR)/$(BINARY)
	@chmod 755 $(BINDIR)/$(BINARY)
	@echo "$(GREEN)Installed $(BINARY) to $(BINDIR)/$(BINARY)$(NC)"

# Uninstall from system
uninstall:
	@echo "$(CYAN)Uninstalling $(BINARY) from $(BINDIR)...$(NC)"
	@rm -f $(BINDIR)/$(BINARY)
	@echo "$(GREEN)Uninstalled $(BINARY)$(NC)"

# Check if Rust is installed
check-rust:
	@which cargo > /dev/null || (echo "$(YELLOW)Rust not found. Install from https://rustup.rs$(NC)" && exit 1)

# Check code without building
check: check-rust
	@echo "$(CYAN)Checking code...$(NC)"
	$(CARGO) check
	@echo "$(GREEN)Check complete$(NC)"

# Format code
fmt: check-rust
	@echo "$(CYAN)Formatting code...$(NC)"
	$(CARGO) fmt
	@echo "$(GREEN)Formatting complete$(NC)"

# Lint code
lint: check-rust
	@echo "$(CYAN)Linting code...$(NC)"
	$(CARGO) clippy -- -D warnings
	@echo "$(GREEN)Lint complete$(NC)"

# Show system info
info:
	@echo "$(CYAN)System Information$(NC)"
	@echo "  OS:           $(OS)"
	@echo "  Architecture: $(ARCH)"
	@echo "  Target:       $(TARGET)"
	@echo "  Install dir:  $(BINDIR)"
	@echo ""
	@echo "$(CYAN)Rust Information$(NC)"
	@which cargo > /dev/null && cargo --version || echo "  Cargo: not installed"
	@which rustc > /dev/null && rustc --version || echo "  Rustc: not installed"

# Run FOP on current directory (for testing)
run: build
	@echo "$(CYAN)Running FOP on current directory...$(NC)"
	$(TARGET_DIR)/$(BINARY) .

# Create distributable archive
dist: build
	@echo "$(CYAN)Creating distribution archive...$(NC)"
	@mkdir -p dist
	@cp $(TARGET_DIR)/$(BINARY) dist/$(BINARY)-$(OS)-$(ARCH)
	@tar -czvf dist/$(BINARY)-$(OS)-$(ARCH).tar.gz -C dist $(BINARY)-$(OS)-$(ARCH)
	@echo "$(GREEN)Created dist/$(BINARY)-$(OS)-$(ARCH).tar.gz$(NC)"

# Help
help:
	@echo "$(CYAN)FOP - Filter Orderer and Preener$(NC)"
	@echo ""
	@echo "$(YELLOW)Usage:$(NC)"
	@echo "  make [target]"
	@echo ""
	@echo "$(YELLOW)Targets:$(NC)"
	@echo "  $(GREEN)build$(NC)     - Build release binary (default)"
	@echo "  $(GREEN)debug$(NC)     - Build debug binary"
	@echo "  $(GREEN)test$(NC)      - Run tests"
	@echo "  $(GREEN)install$(NC)   - Install to $(BINDIR)"
	@echo "  $(GREEN)uninstall$(NC) - Remove from $(BINDIR)"
	@echo "  $(GREEN)clean$(NC)     - Remove build artifacts"
	@echo "  $(GREEN)check$(NC)     - Check code without building"
	@echo "  $(GREEN)fmt$(NC)       - Format code"
	@echo "  $(GREEN)lint$(NC)      - Run clippy linter"
	@echo "  $(GREEN)dist$(NC)      - Create distributable archive"
	@echo "  $(GREEN)info$(NC)      - Show system and Rust info"
	@echo "  $(GREEN)run$(NC)       - Build and run on current directory"
	@echo "  $(GREEN)help$(NC)      - Show this help"
	@echo ""
	@echo "$(YELLOW)Options:$(NC)"
	@echo "  PREFIX=/path  - Set install prefix (default: /usr/local)"
	@echo ""
	@echo "$(YELLOW)Examples:$(NC)"
	@echo "  make                    # Build release binary"
	@echo "  make install            # Install to /usr/local/bin"
	@echo "  sudo make install       # Install system-wide"
	@echo "  make PREFIX=~/.local install  # Install to ~/.local/bin"
	@echo "  make test               # Run tests"
	@echo "  make clean              # Clean build files"
