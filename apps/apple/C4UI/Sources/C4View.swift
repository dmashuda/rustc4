import C4FFI
import SwiftUI
import UniformTypeIdentifiers

/// The viewer's window: a renderer and nothing more. It draws the Rust
/// `Screen` (the views, the diagram, the panel about what is selected, the
/// problems, the source) and reports taps and edits. Every word shown comes
/// from the screen; what a click does is decided in Rust.
public struct C4View: View {
    @State private var model: C4Model
    @State private var editorWidth: CGFloat = 400
    @State private var detailsWidth: CGFloat = 300

    public init(model: C4Model) {
        _model = State(initialValue: model)
    }

    private var screen: Screen { model.screen }

    public var body: some View {
        NavigationSplitView {
            sidebar
        } detail: {
            content
                .navigationTitle(screen.title)
                #if os(macOS)
                .navigationSubtitle(screen.description)
                #endif
                .toolbar { toolbar }
        }
        .dropDestination(for: URL.self) { urls, _ in
            model.open(urls)
            return true
        }
    }

    // MARK: Views

    private var sidebar: some View {
        List(selection: Binding(get: { screen.views.first(where: \.selected)?.id }, set: { id in id.map(model.tap) })) {
            SwiftUI.Section(screen.viewsTitle) {
                ForEach(screen.views, id: \.id) { view in
                    Text(view.text).help(view.hint).accessibilityIdentifier(view.id)
                }
            }
        }
        .safeAreaInset(edge: .bottom) {
            Text(screen.status)
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(12)
                .accessibilityIdentifier("status")
        }
        .navigationSplitViewColumnWidth(min: 170, ideal: 210, max: 320)
    }

    // MARK: The middle

    // Plain stacks with a divider to drag, rather than HSplitView and
    // .inspector: nested inside a NavigationSplitView those two fight
    // AppKit over the window's constraints, and the window can lose
    // ("more Update Constraints in Window passes than there are views").
    private var content: some View {
        HStack(spacing: 0) {
            if let editor = screen.editor {
                SourceEditor(editor: editor, generation: model.generation, screen: screen, model: model)
                    .frame(width: editorWidth)
                DragDivider(width: $editorWidth, range: 240...720, growsToTheRight: true)
            }
            VStack(spacing: 0) {
                stage
                if screen.editor == nil, !screen.problems.isEmpty {
                    Divider()
                    ProblemList(title: screen.problemsTitle, problems: screen.problems, select: nil)
                        .frame(height: 140)
                }
            }
            .frame(minWidth: 240, maxWidth: .infinity, maxHeight: .infinity)
            if let details = screen.details {
                DragDivider(width: $detailsWidth, range: 240...480, growsToTheRight: false)
                DetailsPanel(details: details, tap: model.tap)
                    .frame(width: detailsWidth)
                    .background(.background)
            }
        }
    }

    @ViewBuilder private var stage: some View {
        if let diagram = screen.diagram, screen.empty.isEmpty {
            DiagramCanvas(diagram: diagram, selected: screen.selected, controls: screen.controls, onTap: model.tap)
                .background(Color(nsOrUIColor: .textBackgroundColor))
        } else {
            Text(screen.empty)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .padding(32)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .accessibilityIdentifier("empty")
        }
    }

    @ToolbarContentBuilder private var toolbar: some ToolbarContent {
        ToolbarItem(placement: .navigation) {
            // Always there, so the toolbar does not rearrange itself on
            // every step; with nowhere to go back to, it is disabled.
            Button(action: { if let back = screen.back { model.tap(back.id) } }) {
                Image(systemName: "chevron.backward")
            }
            .disabled(screen.back == nil)
            .help(screen.back?.hint ?? "")
            .accessibilityLabel(screen.back?.text ?? "")
            .accessibilityIdentifier("back")
        }
        ToolbarItemGroup {
            ForEach(screen.exports, id: \.fileName) { export in
                Button(export.text) { save(export) }.help(export.hint)
            }
            #if os(macOS)
            Button(action: { Self.chooseModel(for: model) }) {
                SwiftUI.Label(screen.controls.open, systemImage: "folder")
            }
            .help(screen.controls.open)
            #endif
        }
    }

    // MARK: Files

    #if os(macOS)
    /// Asks for a model: `.c4` files, or a folder of them.
    public static func chooseModel(for model: C4Model) {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = true
        if panel.runModal() == .OK, !panel.urls.isEmpty {
            model.open(panel.urls)
        }
    }

    private func save(_ export: Export) {
        guard let diagram = screen.diagram else { return }
        let panel = NSSavePanel()
        panel.nameFieldStringValue = export.fileName
        panel.allowedContentTypes = [export.kind == .svg ? .svg : .png]
        guard panel.runModal() == .OK, let url = panel.url else { return }
        switch export.kind {
        case .svg:
            try? Data(model.svg().utf8).write(to: url)
        case .png:
            try? Self.png(of: diagram)?.write(to: url)
        }
    }
    #else
    private func save(_ export: Export) {}
    #endif

    /// The diagram as PNG data, at twice its size for sharp text, on a
    /// transparent background: the same drawing the window shows.
    @MainActor static func png(of diagram: Diagram, scale: CGFloat = 2) -> Data? {
        let drawing = Canvas { context, _ in
            DiagramDrawing(diagram: diagram, selected: "", accent: .accentColor).draw(in: &context)
        }
        .frame(width: diagram.width, height: diagram.height)
        let renderer = ImageRenderer(content: drawing)
        renderer.scale = scale
        guard let image = renderer.cgImage else { return nil }
        let data = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(data, UTType.png.identifier as CFString, 1, nil)
        else { return nil }
        CGImageDestinationAddImage(destination, image, nil)
        return CGImageDestinationFinalize(destination) ? data as Data : nil
    }
}

extension Color {
    #if os(macOS)
    init(nsOrUIColor color: NSColor) { self.init(nsColor: color) }
    #else
    init(nsOrUIColor color: UIColor) { self.init(uiColor: color) }
    #endif
}

#if os(iOS)
extension UIColor {
    static var textBackgroundColor: UIColor { .systemBackground }
}
#endif

/// A line between two panes that can be dragged to resize the one whose
/// width it is given.
private struct DragDivider: View {
    @Binding var width: CGFloat
    let range: ClosedRange<CGFloat>
    /// Whether dragging right makes the pane wider (it is on the left).
    let growsToTheRight: Bool

    @State private var start: CGFloat?

    var body: some View {
        Divider()
            .overlay(
                Color.clear
                    .frame(width: 9)
                    .contentShape(Rectangle())
                    .gesture(
                        DragGesture(minimumDistance: 1, coordinateSpace: .global)
                            .onChanged { value in
                                let from = start ?? width
                                start = from
                                let moved = growsToTheRight ? value.translation.width : -value.translation.width
                                width = min(range.upperBound, max(range.lowerBound, from + moved))
                            }
                            .onEnded { _ in start = nil }
                    )
                    #if os(macOS)
                    .onHover { inside in
                        if inside { NSCursor.resizeLeftRight.push() } else { NSCursor.pop() }
                    }
                    #endif
            )
    }
}

// MARK: - The panel about what is selected

private struct DetailsPanel: View {
    let details: Details
    let tap: (String) -> Void

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 10) {
                HStack(alignment: .firstTextBaseline) {
                    Text(details.title).font(.title3.bold()).textSelection(.enabled)
                    Spacer()
                    Button(details.close.text) { tap(details.close.id) }
                        .controlSize(.small)
                        .accessibilityIdentifier(details.close.id)
                }
                Text(details.subtitle).font(.callout.monospaced()).foregroundStyle(.secondary).textSelection(.enabled)
                if !details.technology.isEmpty {
                    Text(details.technology).font(.callout).foregroundStyle(.secondary)
                }
                if !details.description.isEmpty {
                    Text(details.description).textSelection(.enabled)
                }
                if !details.tags.isEmpty {
                    Text(details.tags.map { "#\($0)" }.joined(separator: "  "))
                        .font(.callout)
                        .foregroundStyle(Color.accentColor)
                }
                ForEach(details.links, id: \.url) { link in
                    if let url = URL(string: link.url) {
                        SwiftUI.Link(link.text, destination: url)
                    } else {
                        Text(link.text)
                    }
                }
                ForEach(details.actions, id: \.id) { action in
                    Button(action: { tap(action.id) }) {
                        Text(action.text).frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderedProminent)
                    .help(action.hint)
                    .accessibilityIdentifier(action.id)
                }
                ForEach(details.sections, id: \.title) { section in
                    Text(section.title.uppercased())
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                        .padding(.top, 8)
                    ForEach(Array(section.rows.enumerated()), id: \.offset) { _, row in
                        // A row with no id is something to read, not to tap.
                        if row.id.isEmpty {
                            RowText(row: row)
                        } else {
                            Button(action: { tap(row.id) }) { RowText(row: row) }
                                .buttonStyle(.plain)
                                .accessibilityIdentifier(row.id)
                        }
                    }
                }
            }
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

private struct RowText: View {
    let row: Item

    var body: some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(row.text)
            if !row.detail.isEmpty {
                Text(row.detail).font(.callout).foregroundStyle(.secondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
        .help(row.hint)
    }
}

// MARK: - The source and its problems

private struct SourceEditor: View {
    let editor: Editor
    /// Changes when the text was replaced from outside.
    let generation: Int
    let screen: Screen
    let model: C4Model

    /// What is being typed. Rust is told of every change, but never
    /// writes back into it: only another file or another model does.
    @State private var draft = ""

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                if editor.files.count > 1 {
                    Picker(editor.title, selection: Binding(get: { editor.file }, set: { model.tap("file.\($0)") })) {
                        ForEach(Array(editor.files.enumerated()), id: \.offset) { index, file in
                            Text(file.text).tag(UInt32(index))
                        }
                    }
                    .labelsHidden()
                } else if let only = editor.files.first {
                    Text(only.text).foregroundStyle(.secondary)
                }
                Spacer()
                if let remove = editor.remove {
                    Button(remove.text) { model.tap(remove.id) }
                        .help(remove.hint)
                        .accessibilityIdentifier(remove.id)
                }
                Button(editor.add.text) { model.tap(editor.add.id) }
                    .help(editor.add.hint)
                    .accessibilityLabel(editor.add.hint)
                    .accessibilityIdentifier(editor.add.id)
            }
            .controlSize(.small)
            .padding(8)
            TextEditor(text: $draft)
                .font(.system(size: 12.5, design: .monospaced))
                .autocorrectionDisabled()
                .accessibilityIdentifier("source")
                .accessibilityLabel(editor.title)
                .onChange(of: draft) { _, text in
                    if text != editor.text { model.edit(file: editor.file, text: text) }
                }
            if !screen.problems.isEmpty {
                Divider()
                ProblemList(title: screen.problemsTitle, problems: screen.problems, select: nil)
                    .frame(maxHeight: 160)
            }
        }
        .onAppear { draft = editor.text }
        .onChange(of: editor.file) { draft = editor.text }
        .onChange(of: editor.files.map(\.text)) { draft = editor.text }
        .onChange(of: generation) { draft = editor.text }
    }
}

private struct ProblemList: View {
    let title: String
    let problems: [Problem]
    let select: ((Problem) -> Void)?

    var body: some View {
        List {
            SwiftUI.Section(title) {
                ForEach(Array(problems.enumerated()), id: \.offset) { _, problem in
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Text("\(problem.line):\(problem.col)")
                            .font(.callout.monospaced())
                            .foregroundStyle(problem.error ? Color.red : Color.orange)
                        Text(problem.text).textSelection(.enabled)
                    }
                    .help(problem.file)
                    .onTapGesture { select?(problem) }
                }
            }
        }
        .accessibilityIdentifier("problems")
    }
}
