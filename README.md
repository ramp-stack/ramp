# RAMP

RAMP connects Maverick OS hardware, AIR services, and Prism's application interfaces.

## Building

Maverick builds FFmpeg automatically through Cargo and links its libraries statically into the
application. FFmpeg 9.0.2 source ships in the transitive binding crate and is compiled for the
target. An installed FFmpeg package or runtime is not required. Building FFmpeg requires
neither Git nor a separate source download.

macOS and iOS builds use pregenerated FFmpeg bindings and require Rust, Xcode's native compiler
and target SDK, and Make. NASM is built automatically from bundled source only for Intel
(`x86`/`x86_64`) targets, preserving FFmpeg's optimized assembly. No separate NASM installation
or build-time source download is needed. These builds do not require libclang or pkg-config.
Subsequent builds reuse Cargo's build cache.

Run the application demo with:

```sh
cargo run --features demo
```
