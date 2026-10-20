# Manual CI — run before pushing
#
#  make ci          full pipeline
#  make test-all    both backends
#  make test-sqlite sqlite only
#  make lint        clippy + fmt

.PHONY: ci test-all test-memory test-sqlite test-evil lint fmt clippy

ci: lint test-all

lint: fmt clippy

fmt:
	cargo fmt --all --check

clippy:
	cargo clippy --workspace -- -D warnings

test-all: test-memory test-sqlite

test-memory:
	STORE=memory cargo test --workspace

test-sqlite:
	STORE=sqlite cargo test --workspace

test-evil:
	cargo test --test evil -- --nocapture

# Run inside a Linux Docker container to validate cross-platform.
# First run is slow (~2 min); subsequent runs use cached deps and rebuild only changes.
docker-test:
	docker run --rm \
		-v $(PWD):/app \
		-v cargo-registry:/usr/local/cargo/registry \
		-v $(PWD)/target-docker:/app/target \
		-w /app \
		rust:latest \
		sh -c "cargo test --workspace && STORE=sqlite cargo test --workspace"

docker-lint:
	docker run --rm -v $(PWD):/app -w /app rust:latest \
		sh -c "rustup component add clippy rustfmt && cargo fmt --all --check && cargo clippy --workspace -- -D warnings"
