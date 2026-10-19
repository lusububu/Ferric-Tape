# Building the Tape Machine VST3 on Windows

This can't be cross-compiled from the sandbox it was written in, so these steps run on your
own Windows machine. It's a one-time setup; after that, rebuilding is a single command.

## 1. Install the tools (one time)

1. **Rust**: go to https://rustup.rs, download `rustup-init.exe`, run it, accept the defaults.
   This installs the MSVC toolchain, which needs the next step.
2. **C++ build tools**: install "Build Tools for Visual Studio" from
   https://visualstudio.microsoft.com/visual-cpp-build-tools/ — in the installer, check
   **"Desktop development with C++"** and install just that workload. (This is what actually
   compiles/links the plugin; Rust alone can't link on Windows without it.)
3. Open a **new** terminal (PowerShell or cmd) after both installs finish, so the updated PATH
   takes effect.
4. Install the bundler:
   ```
   cargo install --git https://github.com/robbert-vdh/nih-plug.git cargo-nih-plug
   ```

## 2. Build the plugin

1. Unzip the project you got from me somewhere, e.g. `C:\dev\tape-machine-vst3`.
2. Open a terminal in that folder (`cd C:\dev\tape-machine-vst3`).
3. Run:
   ```
   cargo nih-plug bundle tape-machine-vst3 --release
   ```
   First build takes a few minutes (it's compiling the whole dependency tree); later ones are fast.
4. You'll get:
   ```
   target\bundled\tape-machine-vst3.vst3   <- a folder, not a single file
   target\bundled\tape-machine-vst3.clap
   ```

## 3. Install it

Easiest: run `install.ps1` (included alongside this file) from that same terminal:
```
powershell -ExecutionPolicy Bypass -File install.ps1
```
It copies the built `.vst3` into `C:\Program Files\Common Files\VST3\` and the `.clap` into
`C:\Program Files\Common Files\CLAP\` (the standard system-wide locations every Windows DAW
scans), and tells you if something's missing.

Or do it by hand: copy `target\bundled\tape-machine-vst3.vst3` (the whole folder) into
`C:\Program Files\Common Files\VST3\`.

## 4. Use it

Open your DAW, force a plugin rescan if it doesn't pick it up automatically (most DAWs do this
on next launch, or have a "rescan plugins" button in preferences), and look for **"Tape
Machine"** under effects/FX. CLAP hosts (Bitwig, REAPER, FL Studio) will find it the same way
via the `.clap` copy.

## If the build fails

- `cargo nih-plug bundle tape-machine-vst3 --release` complaining about a linker almost always
  means step 1.2 (the C++ build tools) didn't install, or you're in a terminal opened before it
  finished.
- If `cargo install --git ...` fails to fetch, check your network/proxy isn't blocking
  `github.com` or `crates.io` (it needs both).
