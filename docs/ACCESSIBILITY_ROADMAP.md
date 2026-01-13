# Accessibility Roadmap: Chromium Parity & Beyond

> Goal: Full WCAG 2.1 AAA compliance out of the box

## Current State ✅
- ARIA support, accessibility tree
- Screen reader integration, focus management
- Keyboard navigation, high contrast, reduced motion

---

## Phase 1: Accessibility Tree (Q1)

### 1.1 Tree Construction
| Feature | Chromium | fOS Status | Action |
|---------|----------|------------|--------|
| Full tree | Yes | ✅ | Optimize |
| Role mapping | Complete | Partial | Complete |
| State/properties | Complete | Partial | Complete |
| Live regions | Yes | ❌ | Implement |

```rust
pub struct AccessibilityNode {
    role: AriaRole,
    name: Option<String>,
    description: Option<String>,
    state: AccessibilityState,
    children: Vec<AccessibilityNodeId>,
    bounds: Rect,
}

pub struct AccessibilityTree {
    nodes: SlotMap<AccessibilityNodeId, AccessibilityNode>,
    root: AccessibilityNodeId,
    
    pub fn update_from_dom(&mut self, dom: &DomTree) {
        // Incremental update on DOM changes
    }
}
```

### 1.2 Platform Integration
| Platform | API | Status |
|----------|-----|--------|
| Linux | AT-SPI2 | ❌ Implement |
| macOS | NSAccessibility | ❌ Implement |
| Windows | UIA | ❌ Implement |

---

## Phase 2: Screen Reader Support (Q2)

### 2.1 ARIA Complete Support
| ARIA Feature | Status |
|--------------|--------|
| Roles (all 82) | Partial → Complete |
| States | Partial → Complete |
| Properties | Partial → Complete |
| Live regions | ❌ → Implement |
| Relationships | Partial → Complete |

### 2.2 Announcements
```rust
pub struct LiveRegion {
    politeness: Politeness,  // Off, Polite, Assertive
    atomic: bool,
    relevant: Relevant,       // Additions, Removals, Text, All
    
    pub fn announce(&self, message: &str) {
        // Queue announcement to screen reader
    }
}
```

---

## Phase 3: Input Accessibility (Q3)

### 3.1 Keyboard Navigation
| Feature | Status | Notes |
|---------|--------|-------|
| Tab order | ✅ | Done |
| Arrow keys | ✅ | Done |
| Focus visible | ✅ | Done |
| Skip links | ❌ | Implement |
| Roving tabindex | ❌ | Implement |

### 3.2 Alternative Input
| Input Method | Status |
|--------------|--------|
| Switch access | ❌ Implement |
| Voice control | ❌ Implement |
| Eye tracking | ❌ Investigate |

---

## Phase 4: Visual Accessibility (Q4)

### 4.1 Color & Contrast
```rust
pub struct ContrastChecker {
    pub fn check_wcag(&self, fg: Color, bg: Color) -> ContrastRatio {
        let ratio = self.calculate_ratio(fg, bg);
        ContrastRatio {
            value: ratio,
            aa_normal: ratio >= 4.5,
            aa_large: ratio >= 3.0,
            aaa_normal: ratio >= 7.0,
            aaa_large: ratio >= 4.5,
        }
    }
}
```

### 4.2 User Preferences
| Preference | CSS Query | Support |
|------------|-----------|---------|
| Reduced motion | prefers-reduced-motion | ✅ |
| High contrast | prefers-contrast | ✅ |
| Color scheme | prefers-color-scheme | ✅ |
| Reduced data | prefers-reduced-data | ❌ Implement |
| Reduced transparency | prefers-reduced-transparency | ❌ Implement |

---

## Phase 5: Surpassing Chromium

### 5.1 Unique Features
| Feature | Description | Chromium? |
|---------|-------------|-----------|
| **Auto-fix** | Suggest ARIA fixes | DevTools only |
| **Contrast auto-adjust** | Fix low contrast | No |
| **Focus prediction** | Pre-render focus targets | No |
| **Reading mode** | Simplified view | Extensions |

---

## Benchmarks Target

| Metric | Chromium | fOS Target |
|--------|----------|------------|
| Tree build time | 10ms | 5ms |
| Update latency | 16ms | 8ms |
| Memory overhead | 20% | 10% |

---

## Dependencies Policy

### Keep
- Platform a11y APIs only

### Custom Implementation
1. Accessibility tree builder
2. ARIA role engine
3. Focus manager
4. Live region handler
