# cerveau

A small website that lets you spin my brain around in WebGPU.

Hand-rolled NIfTI-1 parser in Rust (`client/src/nifti.rs` — ~100 lines, no crate), a wgpu MIP ray-marcher in WGSL (`client/src/shader.wgsl`), all compiled to WebAssembly via `trunk`. Tiny axum server (`server/`) just `ServeDir`s the built files.

## Build

```bash
# 1. Copy the source MRI scan into client/ (gitignored)
./scripts/setup-brain-data.sh

# 2. Build the WASM client (trunk required: cargo install trunk)
cd client && trunk build --release && cd ..

# 3. Build the server
cargo build --release -p server

# 4. Run locally
./target/release/server     # listens on http://127.0.0.1:4006
```

## Deploy on air13

Following the pattern in `~/server/server.md`:

```bash
# Install LaunchDaemon
sudo cp deploy/ink.geiger.brain.plist /Library/LaunchDaemons/
sudo chown root:wheel /Library/LaunchDaemons/ink.geiger.brain.plist
sudo chmod 644 /Library/LaunchDaemons/ink.geiger.brain.plist
sudo launchctl bootstrap system /Library/LaunchDaemons/ink.geiger.brain.plist
```

Add to `/opt/homebrew/etc/Caddyfile`:

```
brain.geiger.ink {
    reverse_proxy localhost:4006
}
```

Then `caddy validate` + hot reload as in the server runbook.

## Controls

- **Drag** — rotate
- **Scroll** — zoom
- Auto-spin until you grab it.
