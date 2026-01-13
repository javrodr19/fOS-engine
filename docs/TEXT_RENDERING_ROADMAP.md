# Text Rendering Roadmap: Chromium Parity & Beyond

> Goal: Production-quality text rendering with zero external shaping libraries

## Current State ✅
- Font loading, glyph atlas
- Text shaping, layout
- Ruby text support

---

## Phase 1: Font Support (Q1)

### 1.1 Font Formats
| Format | Chromium | fOS Status | Action |
|--------|----------|------------|--------|
| TrueType (.ttf) | ✅ | ✅ | Done |
| OpenType (.otf) | ✅ | ✅ | Done |
| WOFF | ✅ | ✅ | Done |
| WOFF2 | ✅ | Partial | Complete Brotli |
| Variable fonts | ✅ | ❌ | Implement |
| COLRv1 (color) | ✅ | ❌ | Implement |

```rust
// Variable font axis support
pub struct VariableFont {
    axes: Vec<FontAxis>,
    instances: Vec<NamedInstance>,
    
    pub fn set_variation(&mut self, tag: Tag, value: f32) {
        // Interpolate outlines based on axis values
    }
}

pub struct FontAxis {
    tag: Tag,           // e.g., b"wght" for weight
    min_value: f32,
    default_value: f32,
    max_value: f32,
}
```

### 1.2 Font Loading
| Feature | Chromium | fOS Target |
|---------|----------|------------|
| System fonts | Platform APIs | Custom enumeration |
| Web fonts | Async loading | ✅ Done |
| Font matching | CSS spec | Improve speed |
| Font fallback | Per-char | Per-cluster |

---

## Phase 2: Text Shaping (Q2)

### 2.1 Custom Shaper (Replace HarfBuzz)
| Feature | HarfBuzz | fOS Status |
|---------|----------|------------|
| OpenType GSUB | Full | Implement |
| OpenType GPOS | Full | Implement |
| Script support | 100+ | Start with 20 |
| Kerning | Full | Implement |
| Ligatures | Full | Implement |

```rust
pub struct TextShaper {
    // OpenType feature tables
    gsub: GsubTable,      // Glyph substitution
    gpos: GposTable,      // Glyph positioning
    
    pub fn shape(&self, text: &str, features: &[Feature]) -> ShapedText {
        let mut glyphs = self.map_to_glyphs(text);
        self.apply_gsub(&mut glyphs, features);
        self.apply_gpos(&mut glyphs);
        ShapedText { glyphs }
    }
}
```

### 2.2 Script Coverage Priority
| Script | Users | Priority |
|--------|-------|----------|
| Latin | 2B+ | P0 |
| Arabic | 400M | P0 |
| Devanagari | 600M | P0 |
| CJK | 1.5B | P0 |
| Cyrillic | 250M | P1 |
| Hebrew | 10M | P1 |
| Thai | 60M | P1 |
| All others | - | P2 |

---

## Phase 3: Text Layout (Q3)

### 3.1 Line Breaking
```rust
pub struct LineBreaker {
    // UAX #14 line breaking
    pub fn find_break_opportunities(&self, text: &str) -> Vec<BreakOpportunity> {
        // Mandatory, allowed, prohibited breaks
    }
    
    // Justify text within width
    pub fn justify(&self, line: &mut TextLine, width: f32) {
        // Distribute space at allowed break points
    }
}
```

### 3.2 Bidi Support
```rust
// UAX #9 bidirectional algorithm
pub struct BidiResolver {
    pub fn resolve(&self, text: &str) -> Vec<BidiRun> {
        // Return runs with direction
        // Handle embeddings, overrides
    }
}
```

---

## Phase 4: Rendering Quality (Q4)

### 4.1 Subpixel Rendering
| Mode | Chromium | fOS Target |
|------|----------|------------|
| Grayscale AA | ✅ | ✅ |
| LCD (RGB) | ✅ | Implement |
| LCD (BGR) | ✅ | Implement |
| Vertical LCD | ✅ | Implement |

### 4.2 Glyph Rasterization
```rust
pub enum GlyphRasterization {
    Bitmap,                     // Pre-rasterized
    SDF { spread: f32 },        // Signed distance field
    Outline { precision: u8 },  // Vector at paint time
}
```

---

## Phase 5: Surpassing Chromium

### 5.1 Unique Optimizations
| Feature | Description | Chromium? |
|---------|-------------|-----------|
| **SDF atlas** | Scale-independent glyphs | Partial |
| **Shaping cache** | Per-word results | Yes |
| **SIMD shaping** | Vectorized lookups | No |
| **Incremental layout** | Update without reshape | Partial |

---

## Benchmarks Target

| Metric | Chromium | fOS Target |
|--------|----------|------------|
| Shape 1000 chars | 1ms | 0.5ms |
| Glyph cache size | 10MB | 5MB |
| Font load time | 20ms | 10ms |

---

## Dependencies Policy

### Remove
- HarfBuzz → custom shaper
- FreeType → custom rasterizer

### Custom Implementation Priority
1. OpenType parser
2. GSUB/GPOS engine
3. Bidi algorithm
4. Line breaker
5. SDF rasterizer
