# fOS Engine Documentation
At this time the project is deprecated + it doesn't work. It was too much work and too ambitious, but I plan to continue it in the future.
This directory contains the development roadmaps and phase documentation for the fOS browser engine.

## Documents

| File | Description |
|------|-------------|
| [PHASES.md](./PHASES.md) | Original development phases (1-7) - All complete |
| [ROADMAP.md](./ROADMAP.md) | Detailed feature roadmap (Phases 1-24) |
| [FUNCTIONAL_ROADMAP.md](./FUNCTIONAL_ROADMAP.md) | Progressive browser implementation plan |

## Status

### Engine Development (Complete)
- ✅ Phase 1-7: Core engine implementation
- ✅ Phase 8-19: Advanced features
- ✅ Phase 20-24: Optimizations

### Functional Browser (In Progress)
- 🔄 Level 1: Text-based sites (Wikipedia)
- ⏳ Level 2: Interactive sites (GitHub)
- ⏳ Level 3: Media sites (YouTube)
- ⏳ Level 4: Web apps (Gmail)
- ⏳ Level 5: Complex SPAs (Twitter)

## Architecture

```
fOS Engine
├── fos-html       # HTML5 parsing
├── fos-css        # CSS parsing & styling
├── fos-dom        # DOM tree
├── fos-layout     # Layout engine
├── fos-render     # Rendering (CPU/GPU)
├── fos-js         # JavaScript (QuickJS)
├── fos-net        # Networking
├── fos-canvas     # Canvas 2D
├── fos-media      # Video/Audio
├── fos-security   # Security policies
├── fos-devtools   # Developer tools
└── fos-engine     # Core integration
```

## RAM Targets

| Scenario | Target |
|----------|--------|
| Engine idle | 15 MB |
| Simple page | 30 MB |
| Complex page | 80 MB |
| 5 tabs | 200 MB |
| 10 tabs | 350 MB |
