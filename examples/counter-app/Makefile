# Makefile for building app for different WASI runtimes
.PHONY: help help-db help-realtime help-matrix help-env db realtime matrix env spin wasmtime clean run fresh db-reset validate-params

DB_BACKENDS := sqlite postgres neon supabase turso mysql redis
REALTIME_BACKENDS := off polling redis
HELP_SUBGOALS := $(filter db realtime matrix env,$(MAKECMDGOALS))

empty :=
space := $(empty) $(empty)
DB_BACKEND_CASE := $(subst $(space),|,$(strip $(DB_BACKENDS)))
REALTIME_BACKEND_CASE := $(subst $(space),|,$(strip $(REALTIME_BACKENDS)))

ifneq ($(origin realime),undefined)
  $(error Unknown parameter `realime`; did you mean `realtime`?)
endif

# Preserve values supplied by the parent shell before loading .env defaults.
# GNU make lets included files override environment-origin variables, so restore
# these after .env is loaded to keep `REDIS_URL=... make ...` working.
ENV_DATABASE_URL := $(if $(filter environment%,$(origin DATABASE_URL)),$(DATABASE_URL))
ENV_DATABASE_AUTH_TOKEN := $(if $(filter environment%,$(origin DATABASE_AUTH_TOKEN)),$(DATABASE_AUTH_TOKEN))
ENV_MYSQL_URL := $(if $(filter environment%,$(origin MYSQL_URL)),$(MYSQL_URL))
ENV_POSTGRES_URL := $(if $(filter environment%,$(origin POSTGRES_URL)),$(POSTGRES_URL))
ENV_NEON_DB_URL := $(if $(filter environment%,$(origin NEON_DB_URL)),$(NEON_DB_URL))
ENV_TURSO_URL := $(if $(filter environment%,$(origin TURSO_URL)),$(TURSO_URL))
ENV_TURSO_AUTH_TOKEN := $(if $(filter environment%,$(origin TURSO_AUTH_TOKEN)),$(TURSO_AUTH_TOKEN))
ENV_SUPABASE_URL := $(if $(filter environment%,$(origin SUPABASE_URL)),$(SUPABASE_URL))
ENV_SUPABASE_SECRET_KEY := $(if $(filter environment%,$(origin SUPABASE_SECRET_KEY)),$(SUPABASE_SECRET_KEY))
ENV_REDIS_URL := $(if $(filter environment%,$(origin REDIS_URL)),$(REDIS_URL))
ENV_REDIS_CHANNEL := $(if $(filter environment%,$(origin REDIS_CHANNEL)),$(REDIS_CHANNEL))

# Load .env file and export variables if it exists first.
# This provides local defaults while still allowing shell env and command-line
# make variables to override them.
ifneq (,$(wildcard .env))
  include .env
endif
ifneq ($(ENV_DATABASE_URL),)
  DATABASE_URL := $(ENV_DATABASE_URL)
endif
ifneq ($(ENV_DATABASE_AUTH_TOKEN),)
  DATABASE_AUTH_TOKEN := $(ENV_DATABASE_AUTH_TOKEN)
endif
ifneq ($(ENV_MYSQL_URL),)
  MYSQL_URL := $(ENV_MYSQL_URL)
endif
ifneq ($(ENV_POSTGRES_URL),)
  POSTGRES_URL := $(ENV_POSTGRES_URL)
endif
ifneq ($(ENV_NEON_DB_URL),)
  NEON_DB_URL := $(ENV_NEON_DB_URL)
endif
ifneq ($(ENV_TURSO_URL),)
  TURSO_URL := $(ENV_TURSO_URL)
endif
ifneq ($(ENV_TURSO_AUTH_TOKEN),)
  TURSO_AUTH_TOKEN := $(ENV_TURSO_AUTH_TOKEN)
endif
ifneq ($(ENV_SUPABASE_URL),)
  SUPABASE_URL := $(ENV_SUPABASE_URL)
endif
ifneq ($(ENV_SUPABASE_SECRET_KEY),)
  SUPABASE_SECRET_KEY := $(ENV_SUPABASE_SECRET_KEY)
endif
ifneq ($(ENV_REDIS_URL),)
  REDIS_URL := $(ENV_REDIS_URL)
endif
ifneq ($(ENV_REDIS_CHANNEL),)
  REDIS_CHANNEL := $(ENV_REDIS_CHANNEL)
endif
export

# Get db/realtime from command-line, fallback to .env, then defaults.

# Initialize db if not defined on command line.
db ?= $(if $(DATABASE_BACKEND),$(DATABASE_BACKEND),sqlite)
realtime ?= $(if $(REALTIME_BACKEND),$(REALTIME_BACKEND),off)

# Map database backend aliases to runtime backend names.
ifeq ($(db), sqlite)
  DATABASE_BACKEND = sqlite
endif
ifeq ($(db), postgres)
  DATABASE_BACKEND = postgres
endif
ifeq ($(db), neon)
  DATABASE_BACKEND = neon
endif
ifeq ($(db), supabase)
  DATABASE_BACKEND = supabase
endif
ifeq ($(db), turso)
  DATABASE_BACKEND = turso
endif
ifeq ($(db), redis)
  DATABASE_BACKEND = redis
endif
ifeq ($(db), mysql)
  DATABASE_BACKEND = mysql
endif

ifeq ($(realtime), off)
  REALTIME_BACKEND = off
endif
ifeq ($(realtime), polling)
  REALTIME_BACKEND = polling
endif
ifeq ($(realtime), redis)
  REALTIME_BACKEND = redis
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
ifeq ($(DATABASE_BACKEND), turso)
  CARGO_FEATURES := ssr,libsql
endif
ifeq ($(DATABASE_BACKEND), supabase)
  CARGO_FEATURES := ssr,supabase
endif
ifeq ($(DATABASE_BACKEND), redis)
  CARGO_FEATURES := ssr,redis
endif
ifeq ($(DATABASE_BACKEND), mysql)
  CARGO_FEATURES := ssr,mysql
endif

SPIN_CARGO_FEATURES := $(CARGO_FEATURES)
WASMTIME_CARGO_FEATURES := $(CARGO_FEATURES)
ifneq (,$(filter redis,$(DATABASE_BACKEND) $(REALTIME_BACKEND)))
  SPIN_CARGO_FEATURES := $(CARGO_FEATURES),spin-redis
  WASMTIME_CARGO_FEATURES := $(CARGO_FEATURES),wasi-redis
endif
ifeq ($(DATABASE_BACKEND), mysql)
  SPIN_CARGO_FEATURES := $(SPIN_CARGO_FEATURES),spin-mysql
  WASMTIME_CARGO_FEATURES := $(WASMTIME_CARGO_FEATURES),wasi-mysql
endif

SPIN_MANIFEST := spin.toml
ifeq ($(REALTIME_BACKEND), redis)
  SPIN_MANIFEST := spin.redis.toml
endif

# Default target
help:
	@if [ -n "$(topic)" ]; then \
		case "$(topic)" in \
			db) $(MAKE) --no-print-directory help-db ;; \
			realtime) $(MAKE) --no-print-directory help-realtime ;; \
			matrix|combos|combinations) $(MAKE) --no-print-directory help-matrix ;; \
			env) $(MAKE) --no-print-directory help-env ;; \
			*) echo "Unknown help topic: $(topic). Use db, realtime, matrix, or env."; exit 2 ;; \
		esac; \
	elif [ -n "$(HELP_SUBGOALS)" ]; then \
		:; \
	else \
		echo "Usage:"; \
		echo "  make <target> [db=<backend>] [realtime=<mode>]"; \
		echo "  make help topic=db|realtime|matrix|env"; \
		echo "  make help-db | make help-realtime | make help-matrix | make help-env"; \
		echo ""; \
		echo "Available targets:"; \
		echo "  make help                    - Show this help"; \
		echo "  make wasmtime                - Build and run with Wasmtime (default db=sqlite JSON files under ./data)"; \
		echo "  make spin                    - Build and run with Spin (default db=sqlite Spin SQLite)"; \
		echo "  make run                     - Alias for make wasmtime"; \
		echo "  make db=<backend> fresh      - Reset selected backend tables/files and exit"; \
		echo "  make clean                   - Clean build artifacts, site output, and ./data"; \
		echo ""; \
		echo "Supported parameter values:"; \
		echo "  db=$(DB_BACKENDS)"; \
		echo "  realtime=$(REALTIME_BACKENDS)"; \
		echo ""; \
		echo "Common examples:"; \
		echo "  make wasmtime db=postgres"; \
		echo "  make wasmtime db=postgres realtime=redis"; \
		echo "  make wasmtime db=neon"; \
		echo "  make wasmtime db=neon realtime=polling"; \
		echo "  make wasmtime db=neon realtime=redis"; \
		echo "  make wasmtime db=supabase"; \
		echo "  make wasmtime db=supabase realtime=redis"; \
		echo "  make wasmtime db=turso"; \
		echo "  make wasmtime db=turso realtime=polling"; \
		echo "  make wasmtime db=turso realtime=redis"; \
		echo "  make spin db=postgres"; \
		echo "  make spin db=postgres realtime=redis"; \
		echo "  make spin db=neon realtime=redis"; \
		echo "  make spin db=supabase"; \
		echo "  make spin db=supabase realtime=redis"; \
		echo "  make spin db=turso realtime=redis"; \
		echo "  make wasmtime db=mysql realtime=redis"; \
		echo "  make spin db=mysql realtime=polling"; \
		echo "  make spin db=mysql realtime=redis"; \
		echo "  make wasmtime db=redis realtime=redis"; \
		echo "  make spin db=redis realtime=redis"; \
		echo "  make db=postgres fresh"; \
		echo "  make db=supabase fresh"; \
		echo "  make db=mysql fresh"; \
		echo ""; \
		echo "Use make help-matrix to see the supported runtime/db/realtime combinations."; \
	fi

help-db:
	@echo "Supported db=<backend> values:"
	@echo "  sqlite   - Wasmtime JSON files under ./data; Spin SQLite host store"
	@echo "  postgres - PostgreSQL using POSTGRES_URL"
	@echo "  neon     - Neon Postgres HTTP SQL using NEON_DB_URL"
	@echo "  supabase - Supabase REST using SUPABASE_URL and SUPABASE_SECRET_KEY"
	@echo "  turso    - Turso/LibSQL Hrana HTTP using TURSO_URL and TURSO_AUTH_TOKEN"
	@echo "  mysql    - MySQL using MYSQL_URL, falling back to DATABASE_URL"
	@echo "  redis    - Redis event/checkpoint/read-model store using REDIS_URL"
	@echo ""
	@echo "Public aliases such as db=psql or db=libsql are not supported; use db=postgres or db=turso."

help-realtime:
	@echo "Supported realtime=<mode> values:"
	@echo "  off     - default; commands update the local browser from the mutation response"
	@echo "  polling - SSE endpoint polls the durable event store for newer events"
	@echo "  redis   - SSE endpoint uses Redis wake queues/pub/sub, then replays durable events"
	@echo ""
	@echo "realtime=redis is supported with every supported db backend:"
	@echo "  make wasmtime db=sqlite realtime=redis"
	@echo "  make spin db=sqlite realtime=redis"
	@echo "  make wasmtime db=postgres realtime=redis"
	@echo "  make spin db=postgres realtime=redis"
	@echo "  make wasmtime db=neon realtime=redis"
	@echo "  make spin db=neon realtime=redis"
	@echo "  make wasmtime db=supabase realtime=redis"
	@echo "  make spin db=supabase realtime=redis"
	@echo "  make wasmtime db=turso realtime=redis"
	@echo "  make spin db=turso realtime=redis"
	@echo "  make wasmtime db=mysql realtime=redis"
	@echo "  make spin db=mysql realtime=redis"
	@echo "  make wasmtime db=redis realtime=redis"
	@echo "  make spin db=redis realtime=redis"
	@echo ""
	@echo "The parameter name is realtime. A typo like realime=redis fails fast."

help-matrix:
	@echo "Supported runtime/db/realtime matrix:"
	@echo "  make wasmtime db=<$(DB_BACKENDS)> realtime=<$(REALTIME_BACKENDS)>"
	@echo "  make spin     db=<$(DB_BACKENDS)> realtime=<$(REALTIME_BACKENDS)>"
	@echo ""
	@echo "Notes:"
	@echo "  db=sqlite uses JSON files under ./data on Wasmtime and Spin SQLite on Spin."
	@echo "  db=redis uses Redis as the durable event/checkpoint/read-model store."
	@echo "  realtime=polling uses the selected durable event store for SSE catch-up."
	@echo "  realtime=redis is a notification/wake transport and can be paired with every supported db backend."
	@echo "  with db=redis, Redis is both the durable store and the notification/wake transport."
	@echo "  fresh supports the same db values and never starts the server."
	@echo ""
	@echo "Examples:"
	@echo "  make wasmtime db=neon realtime=polling"
	@echo "  make wasmtime db=neon realtime=redis"
	@echo "  make spin db=neon realtime=redis"
	@echo "  make wasmtime db=supabase realtime=redis"
	@echo "  make spin db=supabase realtime=redis"
	@echo "  make wasmtime db=turso realtime=polling"
	@echo "  make wasmtime db=turso realtime=redis"
	@echo "  make spin db=turso realtime=redis"
	@echo "  make wasmtime db=mysql realtime=redis"
	@echo "  make spin db=mysql realtime=redis"
	@echo "  make db=postgres fresh"
	@echo "  make db=supabase fresh"
	@echo "  make db=mysql fresh"

help-env:
	@echo "Backend environment variables:"
	@echo "  postgres: POSTGRES_URL"
	@echo "  neon:     NEON_DB_URL"
	@echo "  supabase: SUPABASE_URL, SUPABASE_SECRET_KEY"
	@echo "  turso:    TURSO_URL, TURSO_AUTH_TOKEN"
	@echo "  mysql:    MYSQL_URL (falls back to DATABASE_URL)"
	@echo "  redis:    REDIS_URL, REDIS_CHANNEL (optional)"
	@echo ""
	@echo "db=<backend> and realtime=<mode> override .env for one command."

db: help-db
realtime: help-realtime
matrix: help-matrix
env: help-env

validate-params:
	@case "$(db)" in $(DB_BACKEND_CASE)) ;; *) \
		echo "Error: unsupported db=$(db)."; \
		echo "Supported db values: $(DB_BACKENDS)"; \
		echo "Run 'make help-db' for details."; \
		exit 2; \
	esac
	@case "$(realtime)" in $(REALTIME_BACKEND_CASE)) ;; *) \
		echo "Error: unsupported realtime=$(realtime)."; \
		echo "Supported realtime values: $(REALTIME_BACKENDS)"; \
		echo "Run 'make help-realtime' for details."; \
		exit 2; \
	esac

# Run target alias
run: wasmtime

# Reset active database tables and files
db-reset: validate-params
	@echo "Resetting database backend: $(DATABASE_BACKEND)..."
	@DATABASE_URL_VALUE=""; \
	DATABASE_AUTH_TOKEN_VALUE=""; \
	if [ "$(DATABASE_BACKEND)" = "mysql" ]; then \
		if [ -n "$$MYSQL_URL" ]; then \
			DATABASE_URL_VALUE="$$MYSQL_URL"; \
		elif [ -n "$$DATABASE_URL" ]; then \
			DATABASE_URL_VALUE="$$DATABASE_URL"; \
		else \
			echo "Error: Neither DATABASE_URL nor MYSQL_URL is set for MySQL reset."; \
			exit 1; \
		fi; \
	fi; \
	case "$(DATABASE_BACKEND)" in \
		postgres) DATABASE_URL_VALUE="$$POSTGRES_URL" ;; \
		neon) DATABASE_URL_VALUE="$$NEON_DB_URL" ;; \
		turso) DATABASE_URL_VALUE="$$TURSO_URL"; DATABASE_AUTH_TOKEN_VALUE="$$TURSO_AUTH_TOKEN" ;; \
		supabase) DATABASE_URL_VALUE="$$SUPABASE_URL"; DATABASE_AUTH_TOKEN_VALUE="$$SUPABASE_SECRET_KEY" ;; \
		redis) DATABASE_URL_VALUE="$$REDIS_URL" ;; \
	esac; \
	DATABASE_BACKEND=$(DATABASE_BACKEND) DATABASE_URL="$$DATABASE_URL_VALUE" DATABASE_AUTH_TOKEN="$$DATABASE_AUTH_TOKEN_VALUE" ./scripts/reset_db.sh

# fresh resets the database schema host-side and returns immediately without serving
fresh: db-reset

# Build and run with Spin
spin: validate-params
	@echo "Building for Spin runtime with DB: $(DATABASE_BACKEND), realtime: $(REALTIME_BACKEND)..."
	@mkdir -p .spin
	WASI_RUNTIME=spin DATABASE_BACKEND=$(DATABASE_BACKEND) REALTIME_BACKEND=$(REALTIME_BACKEND) cargo leptos build --release --lib-features hydrate --bin-features $(SPIN_CARGO_FEATURES)
	WASI_RUNTIME=spin DATABASE_BACKEND=$(DATABASE_BACKEND) REALTIME_BACKEND=$(REALTIME_BACKEND) LEPTOS_OUTPUT_NAME=counter_app cargo build --lib --target wasm32-wasip2 --release --no-default-features --features $(SPIN_CARGO_FEATURES)
	@if [ "$(REALTIME_BACKEND)" = "redis" ]; then \
		cargo build --manifest-path redis-trigger/Cargo.toml --target wasm32-wasip2 --release; \
	fi
	@echo "Built: target/wasm32-wasip2/release/counter_app.wasm (for Spin)"
	@echo "Starting Spin..."
	@DATABASE_URL_VALUE=""; \
	DATABASE_AUTH_TOKEN_VALUE=""; \
	REDIS_URL_VALUE="$${REDIS_URL:-redis://127.0.0.1:6379}"; \
	REDIS_CHANNEL_VALUE="$${REDIS_CHANNEL:-counter-events}"; \
	SPIN_VARIABLE_FLAGS=""; \
		case "$(DATABASE_BACKEND)" in \
			postgres) DATABASE_URL_VALUE="$$POSTGRES_URL" ;; \
			mysql) DATABASE_URL_VALUE="$${MYSQL_URL:-$$DATABASE_URL}" ;; \
			neon) DATABASE_URL_VALUE="$$NEON_DB_URL" ;; \
		turso) DATABASE_URL_VALUE="$$TURSO_URL"; DATABASE_AUTH_TOKEN_VALUE="$$TURSO_AUTH_TOKEN" ;; \
		supabase) DATABASE_URL_VALUE="$$SUPABASE_URL"; DATABASE_AUTH_TOKEN_VALUE="$$SUPABASE_SECRET_KEY" ;; \
		redis) DATABASE_URL_VALUE="$$REDIS_URL_VALUE" ;; \
	esac; \
	if [ "$(DATABASE_BACKEND)" = "mysql" ] && [ -z "$$DATABASE_URL_VALUE" ]; then \
		echo "Error: Neither DATABASE_URL nor MYSQL_URL is set for MySQL Spin."; \
		exit 1; \
	fi; \
	if [ "$(REALTIME_BACKEND)" = "redis" ]; then \
		SPIN_VARIABLE_FLAGS="--variable redis_url=$$REDIS_URL_VALUE --variable redis_channel=$$REDIS_CHANNEL_VALUE"; \
	fi; \
	spin up --from $(SPIN_MANIFEST) --env DATABASE_BACKEND=$(DATABASE_BACKEND) --env DATABASE_URL="$$DATABASE_URL_VALUE" --env DATABASE_AUTH_TOKEN="$$DATABASE_AUTH_TOKEN_VALUE" --env REALTIME_BACKEND=$(REALTIME_BACKEND) --env REDIS_URL="$$REDIS_URL_VALUE" --env REDIS_CHANNEL="$$REDIS_CHANNEL_VALUE" $$SPIN_VARIABLE_FLAGS

# Build and run with Wasmtime
wasmtime: validate-params
	@echo "Building for Wasmtime runtime with DB: $(DATABASE_BACKEND), realtime: $(REALTIME_BACKEND)..."
	WASI_RUNTIME=wasmtime DATABASE_BACKEND=$(DATABASE_BACKEND) REALTIME_BACKEND=$(REALTIME_BACKEND) cargo leptos build --release --lib-features hydrate --bin-features $(WASMTIME_CARGO_FEATURES)
	WASI_RUNTIME=wasmtime DATABASE_BACKEND=$(DATABASE_BACKEND) REALTIME_BACKEND=$(REALTIME_BACKEND) LEPTOS_OUTPUT_NAME=counter_app cargo build --lib --target wasm32-wasip2 --release --no-default-features --features $(WASMTIME_CARGO_FEATURES) --target-dir target/wasmtime
	@echo "Built: target/wasmtime/wasm32-wasip2/release/counter_app.wasm (for Wasmtime)"
	@echo "Starting Wasmtime serve..."
	@echo "Serving at http://127.0.0.1:3000"
	@mkdir -p ./data
	@DATABASE_URL_VALUE=""; \
	DATABASE_AUTH_TOKEN_VALUE=""; \
	REDIS_URL_VALUE="$${REDIS_URL:-redis://127.0.0.1:6379}"; \
	REDIS_CHANNEL_VALUE="$${REDIS_CHANNEL:-counter-events}"; \
	case "$(DATABASE_BACKEND)" in \
		postgres) DATABASE_URL_VALUE="$$POSTGRES_URL" ;; \
		mysql) DATABASE_URL_VALUE="$${MYSQL_URL:-$$DATABASE_URL}" ;; \
		neon) DATABASE_URL_VALUE="$$NEON_DB_URL" ;; \
		turso) DATABASE_URL_VALUE="$$TURSO_URL"; DATABASE_AUTH_TOKEN_VALUE="$$TURSO_AUTH_TOKEN" ;; \
		supabase) DATABASE_URL_VALUE="$$SUPABASE_URL"; DATABASE_AUTH_TOKEN_VALUE="$$SUPABASE_SECRET_KEY" ;; \
		redis) DATABASE_URL_VALUE="$$REDIS_URL_VALUE" ;; \
	esac; \
	if [ "$(DATABASE_BACKEND)" = "mysql" ] && [ -z "$$DATABASE_URL_VALUE" ]; then \
		echo "Error: Neither DATABASE_URL nor MYSQL_URL is set for MySQL Wasmtime."; \
		exit 1; \
	fi; \
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
		--env=REALTIME_BACKEND=$(REALTIME_BACKEND) \
		--env=REDIS_URL="$$REDIS_URL_VALUE" \
		--env=REDIS_CHANNEL="$$REDIS_CHANNEL_VALUE" \
		--addr 127.0.0.1:3000 \
		target/wasmtime/wasm32-wasip2/release/counter_app.wasm

# Clean target
clean:
	cargo clean
	rm -rf target/site target/wasmtime ./data
	rm -rf .spin
