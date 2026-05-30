# Liquid Glass

## What it is

Liquid Glass is Apple's design language introduced at WWDC 2025, shipping with macOS 26 (Tahoe) and iOS 26. It is a translucent material that refracts and reflects content beneath it — like frosted glass with real-time lensing.

In SwiftUI, it is accessed via `.glassEffect()` and `.buttonStyle(.glass)`. No external library is needed.

---

## The `ClaudeGlass` modifier

Defined in `Shared/Theme.swift`. Always use this instead of calling `.glassEffect()` directly, so the fallback stays in one place.

```swift
struct ClaudeGlass: ViewModifier {
    func body(content: Content) -> some View {
        if #available(macOS 26, iOS 26, *) {
            content
                .glassEffect(.regular, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        } else {
            content
                .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
                .overlay(
                    RoundedRectangle(cornerRadius: 16, style: .continuous)
                        .strokeBorder(.white.opacity(0.18), lineWidth: 0.5)
                )
        }
    }
}

extension View {
    func claudeGlass() -> some View { modifier(ClaudeGlass()) }
}
```

---

## Where glass is applied in this project

| Surface | Code |
|---|---|
| Whole popover panel | `VStack { … }.claudeGlass()` in `PopoverView` |
| Each Settings section card | `.claudeGlass()` in `SettingsView.sectionCard()` |
| Login header overlay bar | `.claudeGlass()` in `LoginView` |
| Login prompt (unauthenticated popover) | `.claudeGlass()` in `LoginPromptView` |
| iOS session card | `.claudeGlass()` in `IOSMainView.usageCard()` |
| iOS weekly card | `.claudeGlass()` in `IOSMainView.usageCard()` |
| All buttons everywhere | `.buttonStyle(.glass)` |
| iOS widget background | `.containerBackground(.clear, for: .widget)` — system applies glass automatically |
| macOS NSPopover | Applied automatically by the OS on recompile with Xcode 26 |

---

## The golden rule

Glass belongs on the **navigation layer** — floating surfaces above content.

```swift
// ✅ Correct — floating panel
VStack { usageBars }
    .claudeGlass()

// ✅ Correct — floating button
Button("Refresh") { }
    .buttonStyle(.glass)

// ❌ Wrong — content layer
List { rows }
    .claudeGlass()

// ❌ Wrong — full-screen background
Color.clear
    .ignoresSafeArea()
    .claudeGlass()
```

---

## API reference

### Variants

```swift
.glassEffect()                          // .regular variant, capsule shape (default)
.glassEffect(.regular)                  // explicit regular
.glassEffect(.clear)                    // more transparent
.glassEffect(.regular.tint(.purple))    // tinted glass
.glassEffect(.regular.interactive())    // responds to hover/press
```

### Shapes

```swift
.glassEffect(.regular, in: .capsule)
.glassEffect(.regular, in: .circle)
.glassEffect(.regular, in: RoundedRectangle(cornerRadius: 16))
.glassEffect(.regular, in: .rect(cornerRadius: .containerConcentric))
```

### Buttons

```swift
Button("Label") { }
    .buttonStyle(.glass)            // preferred — avoids shape quirks
```

### Grouping multiple glass elements

Use `GlassEffectContainer` when two or more glass surfaces are close enough to merge visually. Without it, adjacent glass elements create a visual seam.

```swift
GlassEffectContainer {
    firstCard.glassEffect()
    secondCard.glassEffect()
}
```

### Morphing between states

```swift
@Namespace var ns

if expanded {
    bigView.glassEffectID("card", in: ns)
} else {
    smallView.glassEffectID("card", in: ns)
}
```

---

## Known issues (as of Xcode 26 beta)

- `.glassEffect(.regular.interactive(), in: RoundedRectangle())` sometimes renders as a capsule. Workaround: use `.buttonStyle(.glass)` for interactive elements.
- Rendering artifacts can appear with `.glassProminent` and `.circle` combined. Avoid that combination.
- Glass cannot sample another glass surface — nested `.glassEffect()` calls do not stack correctly. Use `GlassEffectContainer` instead.

---

## Colors in this project

Named colors must be added to each target's Asset Catalog (`Assets.xcassets`):

| Name | Light | Dark | Used when |
|---|---|---|---|
| `UsageGreen` | `#1D9E75` | `#1D9E75` | usage `< 70%` |
| `UsageAmber` | `#BA7517` | `#BA7517` | usage `70–89%` |
| `UsageRed` | `#E24B4A` | `#E24B4A` | usage `≥ 90%` |

These are referenced via `Double.usageColor` extension in `Theme.swift`:

```swift
extension Double {
    var usageColor: Color {
        switch self {
        case ..<70: return Color("UsageGreen", bundle: nil)
        case ..<90: return Color("UsageAmber", bundle: nil)
        default:    return Color("UsageRed",   bundle: nil)
        }
    }
}
```
