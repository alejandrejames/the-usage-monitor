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
                        .frame(width: geo.size.width * (value / 100), height: 6)
                        .animation(.spring(duration: 0.4), value: value)
                }
            }
            .frame(height: 6)

            Text(resetLabel)
                .font(.system(size: 10))
                .foregroundStyle(.tertiary)
        }
    }
}
