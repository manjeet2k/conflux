#!/usr/bin/env bash
# Runs the Rust tests as REAL Windows executables from WSL2 (WSL interop launches .exe files on
# the Windows host), so Windows-only code (NTFS sparse files, GetAdaptersAddresses, the
# NotifyUnicastIpAddressChange watcher, Zone.Identifier, ...) is exercised natively.
#
# Usage: scripts/test-windows.sh [core|desktop|all]   (default: all)
# Needs: rustup target x86_64-pc-windows-gnu, mingw-w64 (gcc + windres on PATH), WSL interop.
set -euo pipefail
cd "$(dirname "$0")/.."
which=${1:-all}
target=x86_64-pc-windows-gnu

command -v x86_64-w64-mingw32-windres >/dev/null ||
  { echo "x86_64-w64-mingw32-windres not on PATH (install mingw-w64 or add its dir to PATH)"; exit 1; }
cmd.exe /c ver >/dev/null 2>&1 ||
  { echo "WSL interop is unavailable: cannot launch Windows executables from here"; exit 1; }

export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER="${CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER:-$(pwd)/scripts/windows-test/wsl-runner.sh}"

if [[ $which == core || $which == all ]]; then
  echo "== conflux-core (Windows exe)"
  cargo test -p conflux-core --target $target
  echo "== conflux-cli (Windows exe)"
  cargo test -p conflux-cli --target $target
fi

if [[ $which == desktop || $which == all ]]; then
  echo "== conflux-desktop (Windows exe)"
  tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
  cp scripts/windows-test/test.manifest scripts/windows-test/test.rc "$tmp/"
  (cd "$tmp" && x86_64-w64-mingw32-windres test.rc -O coff -o test_manifest.o)
  # --lib only: a plain `cargo test` also links the crate's cdylib, which exceeds the 65535
  # exports limit of the MinGW linker.
  CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS="-C link-arg=$tmp/test_manifest.o" \
    cargo test -p conflux-desktop --lib --target $target
fi
