import SwiftUI

/// Colours for the diff view, resolved per colour scheme.
///
/// The syntax palette is GitHub Dark / GitHub Light — the same values the
/// Tauri client's CSS assigns to core's token classes — so both clients
/// colour identical tokens identically once the re-skin lands.
struct DiffPalette {
    private let isDark: Bool

    init(_ colorScheme: ColorScheme) {
        isDark = colorScheme == .dark
    }

    // MARK: Line rows

    /// Background of an added row.
    var addRowBackground: Color {
        isDark ? Color(0x22C55E, alpha: 0.12) : Color(0x16A34A, alpha: 0.10)
    }

    /// Background of a deleted row.
    var removeRowBackground: Color {
        isDark ? Color(0xEF4444, alpha: 0.12) : Color(0xDC2626, alpha: 0.10)
    }

    /// The `+` glyph in an added row's gutter.
    var addGlyph: Color {
        isDark ? Color(0x22C55E) : Color(0x16A34A)
    }

    /// The `-` glyph in a deleted row's gutter.
    var removeGlyph: Color {
        isDark ? Color(0xEF4444) : Color(0xDC2626)
    }

    /// Backplate behind the changed characters of an added row.
    var intraAddBackground: Color {
        isDark ? Color(0x22C55E, alpha: 0.32) : Color(0x16A34A, alpha: 0.28)
    }

    /// Backplate behind the changed characters of a deleted row.
    var intraRemoveBackground: Color {
        isDark ? Color(0xEF4444, alpha: 0.34) : Color(0xDC2626, alpha: 0.30)
    }

    // MARK: Syntax tokens

    /// Foreground for a token class, or `nil` for classes that take the line's
    /// default foreground (mirroring `render::css_class`, where `Plain`,
    /// `Variable`, and `Punctuation` map to no class). `Strong` and `Emphasis`
    /// are weight/style-only; `Strikethrough` dims like the Tauri client's
    /// muted text.
    func color(for tokenClass: TokenClass) -> Color? {
        switch tokenClass {
        case .plain, .variable, .punctuation, .strong, .emphasis:
            nil
        case .keyword, .operator:
            isDark ? Color(0xFF7B72) : Color(0xD73A49)
        case .string, .raw:
            isDark ? Color(0xA5D6FF) : Color(0x032F62)
        case .comment, .quote:
            isDark ? Color(0x8B949E) : Color(0x6A737D)
        case .function, .attribute, .decorator:
            isDark ? Color(0xD2A8FF) : Color(0x6F42C1)
        case .type:
            isDark ? Color(0xFFA657) : Color(0x005CC5)
        case .number, .constant, .heading:
            isDark ? Color(0x79C0FF) : Color(0x005CC5)
        case .tag:
            isDark ? Color(0x7EE787) : Color(0x22863A)
        case .builtin:
            isDark ? Color(0xFFA657) : Color(0xE36209)
        case .link:
            isDark ? Color(0x79C0FF) : Color(0x0366D6)
        case .strikethrough:
            Color.secondary
        }
    }
}

extension Color {
    /// An sRGB colour from a `0xRRGGBB` literal.
    init(_ rgb: UInt32, alpha: Double = 1.0) {
        self.init(
            .sRGB,
            red: Double((rgb >> 16) & 0xFF) / 255,
            green: Double((rgb >> 8) & 0xFF) / 255,
            blue: Double(rgb & 0xFF) / 255,
            opacity: alpha
        )
    }
}
