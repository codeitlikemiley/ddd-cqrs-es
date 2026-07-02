.DEFAULT_GOAL := help
SHELL := /bin/bash

.PHONY: help version publish example clean
.PHONY: spin wasmtime run --dry-run dry-run

# Convenience aliases used by examples/counter-app passthrough.
EXAMPLE_RUNTIME := $(word 2,$(MAKECMDGOALS))

# `make version` can optionally take `make version X.Y.Z`.
VERSION_ARG := $(word 2,$(MAKECMDGOALS))

# `make publish` can optionally take `--dry-run` / `dry-run`.
PUBLISH_MODE_ARG := $(word 2,$(MAKECMDGOALS))

help:
	@echo "Usage:"
	@echo "  make version [<version>]            bump crate version (auto-increments patch if omitted)"
	@echo "  make publish [dry-run]              run crates.io publish flow (or: make publish -- --dry-run)"
	@echo "  make example <spin|wasmtime|run>    run counter-app example with db/realtime args"
	@echo ""
	@echo "Examples:"
	@echo "  make version"
	@echo "  make version 0.2.1"
	@echo "  make publish"
	@echo "  make publish -- --dry-run"
	@echo "  make publish dry-run"
	@echo "  make example spin db=neon realtime=redis"

version:
	@bash scripts/version.sh $(VERSION_ARG)

publish:
	@if [ "$(PUBLISH_MODE_ARG)" = "--dry-run" ] || [ "$(PUBLISH_MODE_ARG)" = "dry-run" ]; then \
		target="dry-run"; \
	else \
		target="publish"; \
	fi; \
	bash scripts/release-crates-io.sh "$$target"

example:
	@if [ -z "$(EXAMPLE_RUNTIME)" ]; then \
		echo "Error: missing example runtime. Use: make example <spin|wasmtime|run>."; \
		exit 2; \
	fi; \
	$(MAKE) -C examples/counter-app $(EXAMPLE_RUNTIME) db="$(db)" realtime="$(realtime)"

# No-op placeholders so `make version X`, `make publish --dry-run`, and
# `make example spin` don't fail on positional arguments.
spin wasmtime run --dry-run dry-run:
	@:

.DEFAULT:
	@if [ "$(firstword $(MAKECMDGOALS))" = "version" ]; then \
		exit 0; \
	fi
	@if [ "$(firstword $(MAKECMDGOALS))" = "publish" ]; then \
		exit 0; \
	fi
	@if [ "$(firstword $(MAKECMDGOALS))" = "example" ]; then \
		exit 0; \
	fi
	@echo "No rule to make target '$@'." >&2
	@exit 2

clean:
	@$(MAKE) -C examples/counter-app clean
