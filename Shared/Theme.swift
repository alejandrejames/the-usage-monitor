// Theme.swift — Shared design tokens for ClaudeUsage
// Liquid Glass (macOS 26 / iOS 26) first, .ultraThinMaterial fallback below.

import SwiftUI

// MARK: - Usage colour helpers

extension Double {
    /// Returns the semantic colour for a usage percentage (0–100).
    var usageColor: Color {
        switch self {
        case ..<70:  return Color("UsageGreen",  bundle: nil)   // #1D9E75
        case ..<90:  return Color("UsageAmber",  bundle: nil)   // #BA7517
        default:     return Color("UsageRed",    bundle: nil)   // #E24B4A
        }
    }
}

// MARK: - Liquid Glass view modifier

/// Applies .glassEffect() on macOS 26+ / iOS 26+, falls back to
/// .ultraThinMaterial + a subtle border on older OS versions.
struct ClaudeGlass: ViewModifier {
    var shape: AnyShape = AnyShape(RoundedRectangle(cornerRadius: 16, style: .continuous))

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
    /// Claude Usage glass surface — Liquid Glass on macOS/iOS 26, material fallback otherwise.
    func claudeGlass() -> some View {
        modifier(ClaudeGlass())
    }
}

// MARK: - Progress bar

struct UsageBar: View {
    let value: Double        // 0–100
    let label: String
    let resetLabel: String

    /// Width actually rendered. In the app this starts at 0 and springs to
    /// `value` on appear, so the bar fills in each time the popover is shown
    /// (the panel is rebuilt on every open, so otherwise it would just pop in at
    /// its final width). In the widget it is seeded to `value` instead —
    /// WidgetKit renders a static snapshot, where an appear-time animation would
    /// never run and would leave the bar stuck at zero width.
    @State private var displayed: Double

    init(value: Double, label: String, resetLabel: String) {
        self.value      = value
        self.label      = label
        self.resetLabel = resetLabel
        #if WIDGET_EXTENSION
        _displayed = State(initialValue: value)     // static snapshot: no animation
        #else
        _displayed = State(initialValue: 0)         // app: fill in on appear
        #endif
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            HStack {
                Text(label)
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(.secondary)
                Spacer()
                Text(String(format: "%.0f%%", value))
                    .font(.system(size: 13, weight: .semibold, design: .rounded))
                    .foregroundStyle(value.usageColor)
            }

            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule()
                        .fill(.white.opacity(0.12))
                        .frame(height: 6)
                    Capsule()
                        .fill(value.usageColor)
                        .frame(width: geo.size.width * (displayed / 100), height: 6)
                        .animation(.spring(duration: 0.4), value: displayed)
                }
            }
            .frame(height: 6)
            // Fill on appear, then keep following later polls while open.
            // The appear-time change is dispatched to the next runloop pass and
            // wrapped in an explicit withAnimation: set inside onAppear directly,
            // SwiftUI folds it into the view's first render and shows no motion.
            #if !WIDGET_EXTENSION
            .onAppear {
                displayed = 0
                DispatchQueue.main.async {
                    withAnimation(.spring(duration: 0.4)) { displayed = value }
                }
            }
            .onChange(of: value) { _, new in displayed = new }
            #endif

            Text(resetLabel)
                .font(.system(size: 10))
                .foregroundStyle(.tertiary)
        }
    }
}
