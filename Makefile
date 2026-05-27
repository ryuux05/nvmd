SHELL := /bin/sh

MARKDOWN ?= README.md

.PHONY: run sample check test fmt clean help

run:
	cargo run -- $(MARKDOWN)

sample:
	cargo run -- sample.md

check:
	cargo check

test:
	cargo test

fmt:
	cargo fmt

clean:
	cargo clean

help:
	@echo "Targets:"
	@echo "  make run MARKDOWN=README.md  Launch detached preview"
	@echo "  make sample                  Launch sample.md preview"
	@echo "  make check                   Run cargo check"
	@echo "  make test                    Run cargo test"
	@echo "  make fmt                     Format Rust code"
	@echo "  make clean                   Remove cargo build output"
