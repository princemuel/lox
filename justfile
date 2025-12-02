set shell := ["bash", "-cu"]
# set dotenv-load := false

default: test lint

build:
    cargo build

build-release:
    cargo build --release

run:
    cargo run

run-release:
    cargo run --release


test:
    cargo test

lint: clippy fmtcheck

clippy:
    cargo clippy --all-targets --all-features -- -D warnings

resolve:
    cargo clippy --all-targets --all-features --fix --allow-dirty --broken-code

format:
    cargo fmt --all

fmtcheck:
    cargo fmt --all -- --check

check:
    cargo check

rimraf:
    cargo clean

alias b := build
alias t := test
alias r := run-release
alias l := lint
alias c := check
