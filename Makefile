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

.PHONY: dev run check test lint fmt clippy clean

dev: run

run:
	TV7_ENGINE_HOST="$(TV7_ENGINE_HOST)" \
	TV7_ENGINE_PORT="$(TV7_ENGINE_PORT)" \
	TV7_ENGINE_LOG_LEVEL="$(TV7_ENGINE_LOG_LEVEL)" \
	TV7_ENGINE_WORK_DIR="$(TV7_ENGINE_WORK_DIR)" \
	$(CARGO) run -p engine-service

check:
	$(CARGO) check

test:
	$(CARGO) test

fmt:
	$(CARGO) fmt --all

clippy:
	$(CARGO) clippy --all-targets --all-features -- -D warnings

lint:
	$(CARGO) fmt --all
	$(CARGO) clippy --fix --allow-dirty --allow-staged --all-targets --all-features -- -D warnings

clean:
	$(CARGO) clean
