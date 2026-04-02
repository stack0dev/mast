.PHONY: check build build-all release test test-core test-unit bench clippy fmt fmt-check ffi docs clean

check:
	cargo check --workspace

build:
	cargo build --workspace

build-all: build ffi
	cd crates/mast-node && npm run build
	cd benchmarks && npm install

release:
	cargo build --workspace --release

test:
	cargo test --workspace

test-core:
	cargo test -p mast-core

test-unit:
	cargo test -p mast-core --lib

bench:
	cargo bench -p mast-core

clippy:
	cargo clippy --workspace -- -D warnings

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

ffi:
	./scripts/build-mast.sh

docs:
	mdbook build docs

clean:
	cargo clean
