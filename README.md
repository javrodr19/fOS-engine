# fOS Engine

A lightweight browser engine written in Rust, designed for minimal RAM usage while maintaining compatibility with modern web standards.

## Goals

- **Minimal RAM**: Target ~20-30MB per tab for simple pages
- **Fast Startup**: Sub-second cold start
- **Modern Standards**: Full HTML5, CSS3, ES2024 support
- **Embeddable**: Use as a library in any Rust application

## Architecture

```
fos-engine/
├── crates/
│   ├── fos-html/     # HTML5 parser (html5ever wrapper)
│   ├── fos-css/      # CSS parser & cascade (lightningcss)
│   ├── fos-dom/      # DOM tree & APIs
│   ├── fos-layout/   # Layout engine (box model, flexbox, grid)
│   ├── fos-render/   # GPU/CPU rendering (tiny-skia/wgpu)
│   ├── fos-js/       # JavaScript runtime (QuickJS)
│   ├── fos-net/      # Networking & resource loading
│   └── fos-engine/   # Main API that ties everything together
└── examples/         # Demo applications
```

## Building

```bash
cargo build --release
```

## Development Phases

See [PHASES.md](PHASES.md) for the complete development roadmap.

## License

GPL-3.0