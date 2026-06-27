# Makefile for building app for different WASI runtimes
.PHONY: help spin wasmtime clean run fresh db-reset

# Load .env file and export variables if it exists first
# This provides the default settings which can be overridden via command-line db= parameter
ifneq (,$(wildcard .env))
  include .env
  export
endif

# Get db from command-line (db=xyz), fallback to DATABASE_BACKEND from .env, fallback to sqlite.

# Initialize db if not defined on command line.
db ?= $(if $(DATABASE_BACKEND),$(DATABASE_BACKEND),sqlite)

# Map database backend aliases to runtime backend names.
ifeq ($(db), sqlite)
  DATABASE_BACKEND = sqlite
endif
ifeq ($(db), postgres)
  DATABASE_BACKEND = postgres
endif
ifeq ($(db), psql)
  DATABASE_BACKEND = postgres
endif
ifeq ($(db), neon)
  DATABASE_BACKEND = neon
endif
ifeq ($(db), supabase)
  DATABASE_BACKEND = supabase
endif
ifeq ($(db), libsql)
  DATABASE_BACKEND = libsql
endif
ifeq ($(db), turso)
  DATABASE_BACKEND = turso
endif

# Map database backend to fine-grained Cargo features
CARGO_FEATURES := ssr
ifeq ($(DATABASE_BACKEND), sqlite)
  CARGO_FEATURES := ssr,sqlite
endif
ifeq ($(DATABASE_BACKEND), postgres)
  CARGO_FEATURES := ssr,postgres
endif
ifeq ($(DATABASE_BACKEND), neon)
  CARGO_FEATURES := ssr,neon
endif
ifeq ($(DATABASE_BACKEND), libsql)
  CARGO_FEATURES := ssr,libsql
endif
ifeq ($(DATABASE_BACKEND), turso)
  CARGO_FEATURES := ssr,libsql
endif
ifeq ($(DATABASE_BACKEND), supabase)
  CARGO_FEATURES := ssr,supabase
endif

# Default target
help:
	@echo "Available targets:"
	@echo "  make spin     - Build and run with Spin"
	@echo "  make wasmtime - Build and run with Wasmtime"
	@echo "  make clean    - Clean build artifacts"
	@echo ""
	@echo "Database Selection Options:"
	@echo "  make wasmtime db=neon      - Run with Neon Postgres HTTP backend"
	@echo "  make wasmtime db=turso     - Run with Turso LibSQL HTTP backend"
	@echo "  make spin db=postgres      - Run with Spin native Postgres backend"
	@echo "  make db=neon fresh         - Reset active database tables without starting the app"

# Run target alias
run: wasmtime

# Reset active database tables and files
db-reset:
	@echo "Resetting database backend: $(DATABASE_BACKEND)..."
	@DATABASE_URL_VALUE=""; \
	DATABASE_AUTH_TOKEN_VALUE=""; \
	case "$(DATABASE_BACKEND)" in \
		postgres) DATABASE_URL_VALUE="$$POSTGRES_URL" ;; \
		neon) DATABASE_URL_VALUE="$$NEON_DB_URL" ;; \
		libsql|turso) DATABASE_URL_VALUE="$$TURSO_URL"; DATABASE_AUTH_TOKEN_VALUE="$$TURSO_AUTH_TOKEN" ;; \
		supabase) DATABASE_URL_VALUE="$$SUPABASE_URL"; DATABASE_AUTH_TOKEN_VALUE="$$SUPABASE_SECRET_KEY" ;; \
	esac; \
	DATABASE_BACKEND=$(DATABASE_BACKEND) DATABASE_URL="$$DATABASE_URL_VALUE" DATABASE_AUTH_TOKEN="$$DATABASE_AUTH_TOKEN_VALUE" ./scripts/reset_db.sh

# fresh resets the database schema host-side and returns immediately without serving
fresh: db-reset

# Build and run with Spin
spin:
	@echo "Building for Spin runtime with DB: $(DATABASE_BACKEND)..."
	@mkdir -p .spin
	WASI_RUNTIME=spin DATABASE_BACKEND=$(DATABASE_BACKEND) cargo leptos build --release --lib-features hydrate --bin-features $(CARGO_FEATURES)
	WASI_RUNTIME=spin DATABASE_BACKEND=$(DATABASE_BACKEND) LEPTOS_OUTPUT_NAME=counter_app cargo build --lib --target wasm32-wasip2 --release --no-default-features --features $(CARGO_FEATURES)
	@echo "Built: target/wasm32-wasip2/release/counter_app.wasm (for Spin)"
	@echo "Starting Spin..."
	@DATABASE_URL_VALUE=""; \
	DATABASE_AUTH_TOKEN_VALUE=""; \
	case "$(DATABASE_BACKEND)" in \
		postgres) DATABASE_URL_VALUE="$$POSTGRES_URL" ;; \
		neon) DATABASE_URL_VALUE="$$NEON_DB_URL" ;; \
		libsql|turso) DATABASE_URL_VALUE="$$TURSO_URL"; DATABASE_AUTH_TOKEN_VALUE="$$TURSO_AUTH_TOKEN" ;; \
		supabase) DATABASE_URL_VALUE="$$SUPABASE_URL"; DATABASE_AUTH_TOKEN_VALUE="$$SUPABASE_SECRET_KEY" ;; \
	esac; \
	spin up --env DATABASE_BACKEND=$(DATABASE_BACKEND) --env DATABASE_URL="$$DATABASE_URL_VALUE" --env DATABASE_AUTH_TOKEN="$$DATABASE_AUTH_TOKEN_VALUE"

# Build and run with Wasmtime
wasmtime:
	@echo "Building for Wasmtime runtime with DB: $(DATABASE_BACKEND)..."
	WASI_RUNTIME=wasmtime DATABASE_BACKEND=$(DATABASE_BACKEND) cargo leptos build --release --lib-features hydrate --bin-features $(CARGO_FEATURES)
	WASI_RUNTIME=wasmtime DATABASE_BACKEND=$(DATABASE_BACKEND) LEPTOS_OUTPUT_NAME=counter_app cargo build --lib --target wasm32-wasip2 --release --no-default-features --features $(CARGO_FEATURES) --target-dir target/wasmtime
	@echo "Built: target/wasmtime/wasm32-wasip2/release/counter_app.wasm (for Wasmtime)"
	@echo "Starting Wasmtime serve..."
	@echo "Serving at http://127.0.0.1:3000"
	@mkdir -p ./data
	@DATABASE_URL_VALUE=""; \
	DATABASE_AUTH_TOKEN_VALUE=""; \
	case "$(DATABASE_BACKEND)" in \
		postgres) DATABASE_URL_VALUE="$$POSTGRES_URL" ;; \
		neon) DATABASE_URL_VALUE="$$NEON_DB_URL" ;; \
		libsql|turso) DATABASE_URL_VALUE="$$TURSO_URL"; DATABASE_AUTH_TOKEN_VALUE="$$TURSO_AUTH_TOKEN" ;; \
		supabase) DATABASE_URL_VALUE="$$SUPABASE_URL"; DATABASE_AUTH_TOKEN_VALUE="$$SUPABASE_SECRET_KEY" ;; \
	esac; \
	wasmtime serve \
		-W component-model-async=y \
		-S p3=y \
		-S cli=y \
		-S http=y \
		-S tcp=y \
		-S inherit-network=y \
		-S allow-ip-name-lookup=y \
		--dir=./target/site/pkg::/ \
		--dir=./data::/data \
		--env=LEPTOS_OUTPUT_NAME=counter_app \
		--env=LEPTOS_SITE_ROOT=/ \
		--env=LEPTOS_SITE_PKG_DIR=pkg \
		--env=DATABASE_BACKEND=$(DATABASE_BACKEND) \
		--env=DATABASE_URL="$$DATABASE_URL_VALUE" \
		--env=DATABASE_AUTH_TOKEN="$$DATABASE_AUTH_TOKEN_VALUE" \
		--addr 127.0.0.1:3000 \
		target/wasmtime/wasm32-wasip2/release/counter_app.wasm

# Clean target
clean:
	cargo clean
	rm -rf target/site target/wasmtime ./data
	rm -rf .spin
