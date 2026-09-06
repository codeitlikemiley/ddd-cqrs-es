#!/usr/bin/env bash
set -euo pipefail
exec bash "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/examples/fullstack-app/scripts/sync_fullstack_template.sh" "$@"
