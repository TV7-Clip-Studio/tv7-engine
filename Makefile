CORE_DIR := ../tv7-core
ifeq ("$(wildcard $(CORE_DIR))","")
$(error tv7-core not found. Please clone it as a sibling directory: ../tv7-core)
endif

-include Makefile.core

CARGO ?= cargo

TV7_ENGINE_HOST ?= 127.0.0.1
TV7_ENGINE_PORT ?= 7070
TV7_ENGINE_LOG_LEVEL ?= debug
TV7_ENGINE_WORK_DIR ?= $(CURDIR)

.PHONY: dev run check test lint fmt fmt-check clippy audit deny quality clean

dev: run

run:
	TV7_ENGINE_HOST="$(TV7_ENGINE_HOST)" \
	TV7_ENGINE_PORT="$(TV7_ENGINE_PORT)" \
	TV7_ENGINE_LOG_LEVEL="$(TV7_ENGINE_LOG_LEVEL)" \
	TV7_ENGINE_WORK_DIR="$(TV7_ENGINE_WORK_DIR)" \
	$(CARGO) run -p engine-service

check:
	$(CARGO) check --workspace --all-targets

test:
	$(CARGO) test --workspace --all-targets

fmt:
	$(CARGO) fmt --all

fmt-check:
	$(CARGO) fmt --all -- --check

clippy:
	$(CARGO) clippy --workspace --all-targets --all-features -- -D warnings

audit:
	$(CARGO) audit

deny:
	$(CARGO) deny check licenses bans sources

quality: fmt-check check test clippy audit deny

lint:
	$(CARGO) fmt --all
	$(CARGO) clippy --fix --allow-dirty --allow-staged --workspace --all-targets --all-features -- -D warnings

clean:
	$(CARGO) clean
