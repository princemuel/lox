.PHONY: all default build run test lint clippy resolve format rimraf

.DEFAULT_GOAL := default

default: test lint

build:
	cargo build --release

run:
	cargo run --release

test:
	cargo test

lint: clippy format

clippy:
	cargo clippy --all-targets --all-features -- -D warnings

resolve:
	cargo clippy --all-targets --all-features --fix --allow-dirty --broken-code

format:
	cargo fmt --all -- --check

rimraf:
	cargo clean
