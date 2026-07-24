.PHONY: build test run clean docker docker-run deps

BINARY=the-bridge-matching-engine

build:
	cargo build --release

test:
	cargo test --release -- --nocapture

test-stress:
	cargo test --release --test stress_test -- --nocapture

run:
	cargo run --release

clean:
	cargo clean
	rm -rf /var/lib/the-bridge/wal/*

deps:
	apt-get install -y libnuma-dev pkg-config

docker:
	docker compose build

docker-run:
	docker compose up -d

docker-logs:
	docker compose logs -f

docker-stop:
	docker compose down
