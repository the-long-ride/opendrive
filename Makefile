.PHONY: check test self-check fmt clippy

check:
	cargo check --workspace

test:
	cargo test --workspace

self-check:
	cargo run -p opendrive-app -- --self-check

fmt:
	cargo fmt --all

clippy:
	cargo clippy --workspace --all-targets -- -D warnings
