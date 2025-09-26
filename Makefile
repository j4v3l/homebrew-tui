# Makefile for homebrew-tui

CARGO ?= cargo
BINARY = homebrew-tui

.PHONY: all build run run-release test release install fmt clippy check clean brew-check package help

all: build

build:
	$(CARGO) build

run:
	$(CARGO) run

run-release:
	$(CARGO) run --release

test:
	$(CARGO) test

release:
	$(CARGO) build --release

install:
	$(CARGO) install --path .

fmt:
	$(CARGO) fmt --all

clippy:
	$(CARGO) clippy -- -D warnings

check:
	$(CARGO) check

clean:
	$(CARGO) clean

brew-check:
	@command -v brew >/dev/null 2>&1 || { echo "Homebrew not found in PATH. Install from https://brew.sh/"; exit 1; }
	@echo "Homebrew found: $(shell brew --version 2>/dev/null | head -n 1)"

package: release
	@mkdir -p dist
	@tar -czf dist/$(BINARY)-$$(date +%Y%m%d).tar.gz -C target/release $(BINARY) README.md || true
	@echo "Package created in dist/"

help:
	@echo "Makefile targets:";
	@echo "  make build        - debug build";
	@echo "  make run          - run in debug mode";
	@echo "  make run-release  - run optimized release build";
	@echo "  make test         - run unit tests";
	@echo "  make release      - release build";
	@echo "  make install      - cargo install from current path";
	@echo "  make fmt          - run rustfmt";
	@echo "  make clippy       - run clippy (requires clippy)";
	@echo "  make check        - cargo check";
	@echo "  make clean        - cargo clean";
	@echo "  make brew-check   - verify Homebrew is available on PATH";
	@echo "  make package      - build release and create a tarball in dist/";
