import SwiftUI

/// Label above the control, full-width — the field layout shared by the
/// Clone and Publish sheets.
struct VerticalLabeledContentStyle: LabeledContentStyle {
    func makeBody(configuration: Configuration) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            configuration.label
                .font(.callout)
            configuration.content
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

extension LabeledContentStyle where Self == VerticalLabeledContentStyle {
    static var vertical: VerticalLabeledContentStyle { VerticalLabeledContentStyle() }
}
