# Compiling Conflux for Windows from Linux & WSL2

> Part of the [development docs](../DEVELOPMENT.md). Everyday checks (lint, tests) do not need a Windows build; use this only when you need a Windows binary.

This guide outlines how to compile native standalone Windows executables (`.exe`) for Conflux from your Linux/WSL2 environment.

---

## Method 1: Cross-Compiling with MinGW-w64 (Recommended)

This produces a native Windows binary using the GNU ABI (`x86_64-pc-windows-gnu`).

### Step 1: Install MinGW-w64 Linker
In your Linux/WSL terminal, run:
```bash
sudo apt update && sudo apt install -y mingw-w64
```

### Step 2: Ensure the Rust Windows GNU Target is Installed
```bash
rustup target add x86_64-pc-windows-gnu
```

### Step 3: Compile Conflux for Windows
```bash
cargo build --target x86_64-pc-windows-gnu --release
```
The compiled Windows executable will be generated at:
```text
target/x86_64-pc-windows-gnu/release/conflux.exe
```

---

## Method 2: Cross-Compiling with `cargo-xwin` (MSVC ABI)

This produces an MSVC-compatible Windows executable (`x86_64-pc-windows-msvc`) directly from Linux without needing Visual Studio installed.

### Step 1: Install Clang and LLD Linker
```bash
sudo apt install -y clang lld
```

### Step 2: Build with `cargo-xwin`
```bash
cargo xwin build --target x86_64-pc-windows-msvc --release
```
The resulting executable will be generated at:
```text
target/x86_64-pc-windows-msvc/release/conflux.exe
```

---

## Method 3: Compiling from Windows Host (WSL Interop)

Since you are running inside WSL2 on a Windows host, you can also build natively using Windows PowerShell:

1. Open **Windows PowerShell** or **Command Prompt** on Windows.
2. Navigate to your project folder inside WSL:
   ```powershell
   cd \\wsl$\Ubuntu\home\manjeet\conflux
   ```
3. Run native Cargo:
   ```powershell
   cargo build --release
   ```
   The Windows binary will be built natively using your Windows toolchain.

---

## Notes

- `cargo clippy -p conflux-desktop --target x86_64-pc-windows-gnu -- -D warnings` needs the mingw `windres` (`x86_64-w64-mingw32-windres`) on `PATH`, because `tauri-winres` invokes it in the build script. If it is installed outside the default path, prepend that directory to `PATH` for the command.
- Release installers are meant to come from CI, not local builds, once roadmap task R-3 lands (see [ROADMAP.md](../ROADMAP.md)). Until then, build an installer locally only when explicitly requested.

