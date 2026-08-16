import SwiftUI

/// The main pane's tab strip: left-aligned text tabs with an accent underline
/// under the active one — the Tauri tab bar's design, which STYLE.md
/// specifies for both clients. It replaced a stock segmented picker for one
/// structural reason: a segment can only hold text or an image, and the
/// Changes tab carries a count pill.
struct RepoTabBar: View {
    let tabs: [RepoTab]
    @Binding var selection: RepoTab
    /// The working-tree file count shown on the Changes tab — every row the
    /// list shows, untracked and conflicted included, hidden at zero and
    /// never capped, visible from any tab (the Tauri badge's semantics).
    let changesCount: Int

    @State private var hoveredTab: RepoTab?

    var body: some View {
        HStack(spacing: 0) {
            ForEach(tabs) { tab in
                tabButton(tab)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 8)
    }

    private func tabButton(_ tab: RepoTab) -> some View {
        let isActive = tab == selection
        return Button {
            selection = tab
        } label: {
            HStack(spacing: 6) {
                Text(tab.rawValue)
                    .font(.system(size: 13, weight: isActive ? .semibold : .regular))
                    .foregroundStyle(titleStyle(isActive: isActive, isHovered: hoveredTab == tab))
                if tab == .changes, changesCount > 0 {
                    CountBadge(count: changesCount)
                }
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .contentShape(.rect)
            // The 2 pt accent underline sits at the strip's bottom edge, so
            // it visually fuses with the hairline divider drawn below it.
            .overlay(alignment: .bottom) {
                if isActive {
                    Rectangle()
                        .fill(.tint)
                        .frame(height: 2)
                }
            }
        }
        .buttonStyle(.plain)
        .onHover { isInside in
            if isInside {
                hoveredTab = tab
            } else if hoveredTab == tab {
                hoveredTab = nil
            }
        }
    }

    /// Active reads as primary; inactive is muted and brightens on hover —
    /// the three text states of the Tauri tab, in system styles.
    private func titleStyle(isActive: Bool, isHovered: Bool) -> some ShapeStyle {
        if isActive { return AnyShapeStyle(.primary) }
        if isHovered { return AnyShapeStyle(.secondary) }
        return AnyShapeStyle(.tertiary)
    }
}
