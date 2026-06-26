# Makefile for building app for different WASI runtimes
.PHONY: help spin wasmtime clean run migrate-fresh db-reset

# Load .env file and export variables if it exists first
# This provides the default settings which can be overridden via command-line db= parameter
ifneq (,$(wildcard .env))
  include .env
  export
endif

# Get db from command-line (db=xyz or --db=xyz), fallback to DATABASE_BACKEND from .env, fallback to sqlite
DB_ARG := $(filter --db=%,$(MAKECMDGOALS))
ifneq ($(DB_ARG),)
  db := $(patsubst --db=%,%,$(DB_ARG))
  $(eval $(DB_ARG):;@:)
endif

# Support --fresh flag
FRESH_ARG := $(filter --fresh,$(MAKECMDGOALS))
ifneq ($(FRESH_ARG),)
  $(eval $(FRESH_ARG):;@:)
endif

# Initialize db if not defined on command line
db ?= $(DATABASE_BACKEND)
db ?= sqlite

# Map database backend to backend types.
# Note: For WASM/WASI compilation targets, we keep DB_FEATURES empty because standard 
# socket-dependent dependencies (like tokio/mio/postgres native TCP client) do NOT compile on WASM.
# All remote storage backends compile using standard stateless WASIp3 Outbound HTTP queries.
DB_FEATURES =
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

# Build --env flags for wasmtime and spin dynamically based on active DATABASE_BACKEND
ENV_VARS := DATABASE_BACKEND=$(DATABASE_BACKEND)

ifeq ($(DATABASE_BACKEND), postgres)
  ENV_VARS += POSTGRES_URL=$(POSTGRES_URL)
endif

ifeq ($(DATABASE_BACKEND), neon)
  ENV_VARS += POSTGRES_URL=$(POSTGRES_URL)
endif

ifeq ($(DATABASE_BACKEND), supabase)
  ENV_VARS += SUPABASE_URL=$(SUPABASE_URL) SUPABASE_PUBLISHABLE_KEY=$(SUPABASE_PUBLISHABLE_KEY) SUPABASE_SECRET_KEY=$(SUPABASE_SECRET_KEY) SUPABASE_JWKS_URL=$(SUPABASE_JWKS_URL)
endif

ifeq ($(DATABASE_BACKEND), libsql)
  ENV_VARS += TURSO_URL=$(TURSO_URL) TURSO_AUTH_TOKEN=$(TURSO_AUTH_TOKEN)
endif

ifeq ($(DATABASE_BACKEND), turso)
  ENV_VARS += TURSO_URL=$(TURSO_URL) TURSO_AUTH_TOKEN=$(TURSO_AUTH_TOKEN)
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
	@echo "  make wasmtime --db=libsql  - Run with local/remote LibSQL HTTP backend"
	@echo "  make spin db=postgres      - Run with Spin native Postgres backend"
	@echo "  make migrate-fresh         - Reset active database tables and start the app (runs migrations on startup)"

# Run target alias
run: wasmtime

# Reset active database tables and files
db-reset:
	@echo "Resetting database backend: $(DATABASE_BACKEND)..."
ifeq ($(DATABASE_BACKEND), sqlite)
	@rm -rf ./data .spin
	@echo "SQLite database files/directories deleted."
endif
ifeq ($(DATABASE_BACKEND), postgres)
	@if [ -z "$(POSTGRES_URL)" ]; then \
		echo "Error: POSTGRES_URL is not set."; \
		exit 1; \
	fi
	@echo "Dropping PostgreSQL tables..."
	psql "$(POSTGRES_URL)" -c "DROP TABLE IF EXISTS events; DROP TABLE IF EXISTS checkpoints; DROP TABLE IF EXISTS counter_read_model;"
endif
ifeq ($(DATABASE_BACKEND), neon)
	@if [ -z "$(POSTGRES_URL)" ]; then \
		echo "Error: POSTGRES_URL is not set."; \
		exit 1; \
	fi
	@echo "Dropping Neon PostgreSQL tables..."
	psql "$(POSTGRES_URL)" -c "DROP TABLE IF EXISTS events; DROP TABLE IF EXISTS checkpoints; DROP TABLE IF EXISTS counter_read_model;"
endif
ifeq ($(DATABASE_BACKEND), supabase)
	@if [ -z "$(POSTGRES_URL)" ]; then \
		echo "Note: POSTGRES_URL not set. If you configured direct database access, please set POSTGRES_URL to run SQL commands directly."; \
	else \
		echo "Dropping Supabase PostgreSQL tables..."; \
		psql "$(POSTGRES_URL)" -c "DROP TABLE IF EXISTS events; DROP TABLE IF EXISTS checkpoints; DROP TABLE IF EXISTS counter_read_model;"; \
	fi
endif

# Reset database tables and launch the application (Spin by default)
migrate-fresh: db-reset
	@echo "Database reset complete. Starting application to execute schema migrations..."
	$(MAKE) spin

# Build and run with Spin
spin:
ifneq ($(FRESH_ARG),)
	$(MAKE) db-reset
endif
	@echo "Building for Spin runtime with DB: $(DATABASE_BACKEND) ($(DB_FEATURES))..."
	@mkdir -p .spin
	WASI_RUNTIME=spin DATABASE_BACKEND=$(DATABASE_BACKEND) cargo leptos build --release $(DB_FEATURES)
	WASI_RUNTIME=spin DATABASE_BACKEND=$(DATABASE_BACKEND) LEPTOS_OUTPUT_NAME=counter_app cargo build --lib --target wasm32-wasip2 --release --no-default-features --features ssr$(if $(DB_FEATURES),,)$(patsubst --features %,%,$(DB_FEATURES))
	@echo "Built: target/wasm32-wasip2/release/counter_app.wasm (for Spin)"
	@echo "Starting Spin..."
	spin up $(foreach val,$(ENV_VARS),--env '$(val)')

# Build and run with Wasmtime
wasmtime:
ifneq ($(FRESH_ARG),)
	$(MAKE) db-reset
endif
	@echo "Building for Wasmtime runtime with DB: $(DATABASE_BACKEND) ($(DB_FEATURES))..."
	WASI_RUNTIME=wasmtime DATABASE_BACKEND=$(DATABASE_BACKEND) cargo leptos build --release $(DB_FEATURES)
	WASI_RUNTIME=wasmtime DATABASE_BACKEND=$(DATABASE_BACKEND) LEPTOS_OUTPUT_NAME=counter_app cargo build --lib --target wasm32-wasip2 --release --no-default-features --features ssr$(if $(DB_FEATURES),,)$(patsubst --features %,%,$(DB_FEATURES)) --target-dir target/wasmtime
	@echo "Built: target/wasmtime/wasm32-wasip2/release/counter_app.wasm (for Wasmtime)"
	@echo "Starting Wasmtime serve..."
	@echo "Serving at http://127.0.0.1:3000"
	@mkdir -p ./data
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
		$(foreach val,$(ENV_VARS),--env '$(val)') \
		--addr 127.0.0.1:3000 \
		target/wasmtime/wasm32-wasip2/release/counter_app.wasm

# Clean target
clean:
	cargo clean
	rm -rf target/site target/wasmtime ./data
	rm -rf .spin
