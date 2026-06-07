#!/usr/bin/env bash
set -euo pipefail

pnpm verify:apps
pnpm verify:core
