.PHONY: check test build clippy audit fmt docker

check:
	cargo check --workspace

test:
	cargo test --workspace

clippy:
	cargo clippy --workspace -- -D warnings

audit:
	cargo audit

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

build:
	cargo build --release --bin agentforge-server --bin agentforge-sidecar --bin agentforge-orchestrator --bin agentforge

docker:
	docker build -t agentforge-server:latest -f Dockerfile .
	docker build -t agentforge-sidecar:latest -f Dockerfile.sidecar .
	docker build -t agentforge-orchestrator:latest -f Dockerfile.orchestrator .

ci: fmt-check clippy test audit
	@echo "All CI checks passed"

clean:
	cargo clean
