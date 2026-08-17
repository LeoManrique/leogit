import AppKit

/// The one place the app writes to the system pasteboard.
///
/// `NSPasteboard.general` needs clearing before every write — without it the
/// new value is added to whatever the previous owner declared, and readers can
/// pick up the stale flavour. Wrapping it here keeps that pairing from being
/// re-derived at each call site (the copy items in the file and commit row
/// menus, and the commit header's copy-SHA button).
enum Clipboard {
    static func copy(_ text: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }
}
