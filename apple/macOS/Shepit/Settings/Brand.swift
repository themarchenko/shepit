import SwiftUI

/// Shepit's mark, "voice to cursor": three rising bars flowing into a text caret. Same geometry as the app
/// icon drawn by scripts/make-icons.swift (a 100 x 100 box); change both together.
struct ShepitMark: View {
    var color: Color = WidgetPalette.text

    private static let pieces: [(x: CGFloat, y: CGFloat, w: CGFloat, h: CGFloat, alpha: Double)] = [
        (18, 45, 5, 10, 0.35), (27, 40, 5, 20, 0.65), (36, 34.5, 5, 31, 0.95),
        (57, 24, 6, 52, 1), (47, 21, 26, 6, 1), (47, 73, 26, 6, 1),
    ]

    var body: some View {
        Canvas { context, size in
            let s = min(size.width, size.height) / 100
            for p in Self.pieces {
                let rect = CGRect(x: p.x * s, y: p.y * s, width: p.w * s, height: p.h * s)
                context.fill(Path(roundedRect: rect, cornerRadius: min(rect.width, rect.height) / 2),
                             with: .color(color.opacity(p.alpha)))
            }
        }
        .aspectRatio(1, contentMode: .fit)
        .accessibilityHidden(true)
    }
}

/// The mark next to the name, as in the tour header.
struct ShepitLockup: View {
    var size: CGFloat = 22

    var body: some View {
        HStack(spacing: size * 0.3) {
            ShepitMark().frame(width: size, height: size)
            Text("Shepit").font(.system(size: size * 0.68, weight: .semibold)).tracking(-0.2)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Shepit")
    }
}
