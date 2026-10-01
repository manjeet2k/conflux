#!/usr/bin/env bash
# Feeds /dev/null to Windows test binaries invoked via WSL binfmt_misc so they do
# not stall waiting for interactive console/stdin pipes.
"$@" < /dev/null
