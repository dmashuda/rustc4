import C4FFI
import Foundation
import Observation

/// Holds the Rust `C4Ui` and its latest `Screen` for SwiftUI.
///
/// Nothing here knows what is on the screen: views draw `screen` and call
/// `tap` / `edit` with what the user did. What is here is what only the
/// platform can do: reading a model's files from disk, and noticing when
/// they change.
@MainActor
@Observable
public final class C4Model {
    public private(set) var screen: Screen
    /// Goes up whenever the sources are replaced from outside (a model is
    /// opened, a file changes on disk): an editor showing the old text
    /// must take the new.
    public private(set) var generation = 0

    private let ui: C4Ui
    /// What was opened: files and directories, re-read when they change.
    @ObservationIgnored private var roots: [URL] = []
    @ObservationIgnored private var fingerprint = ""
    @ObservationIgnored private var timer: Timer?

    /// Opens on the example model, with its source there to edit.
    public init(ui: C4Ui = .example()) {
        self.ui = ui
        screen = ui.screen()
    }

    // MARK: What the user did

    public func tap(_ id: String) {
        screen = ui.tap(id: id)
    }

    public func edit(file: UInt32, text: String) {
        screen = ui.setSource(file: file, text: text)
    }

    /// The showing diagram as SVG text, for saving.
    public func svg(background: String? = nil) -> String {
        ui.svg(background: background)
    }

    // MARK: Models on disk

    /// Shows the model in these files and directories, and keeps showing
    /// it as they change: edit in any editor, and the window follows.
    public func open(_ urls: [URL], watching: Bool = true) {
        roots = urls
        fingerprint = ""
        reloadIfChanged()
        timer?.invalidate()
        guard watching else { return }
        timer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.reloadIfChanged() }
        }
    }

    /// Re-reads the opened files if any was added, removed or saved.
    public func reloadIfChanged() {
        guard !roots.isEmpty else { return }
        let files = Self.sourceFiles(in: roots)
        let now = files.map { "\($0.url.path)@\($0.modified.timeIntervalSince1970)" }.joined(separator: "\n")
        guard now != fingerprint else { return }
        fingerprint = now
        let sources = files.compactMap { file in
            (try? String(contentsOf: file.url, encoding: .utf8)).map { Source(name: file.name, text: $0) }
        }
        screen = ui.setSources(sources: sources, editable: false)
        generation += 1
    }

    /// Every `.c4` and `.likec4` file under the roots, and every `.svg`
    /// (an icon's image, which the model takes as one more source), in a
    /// fixed order, named by its path below the root it was found under.
    static func sourceFiles(in roots: [URL]) -> [(url: URL, name: String, modified: Date)] {
        let manager = FileManager.default
        let keys: [URLResourceKey] = [.contentModificationDateKey, .isRegularFileKey]
        let modified = { (url: URL) in
            (try? url.resourceValues(forKeys: Set(keys)).contentModificationDate) ?? .distantPast
        }
        var found: [(url: URL, name: String, modified: Date)] = []
        for root in roots {
            var isDirectory: ObjCBool = false
            guard manager.fileExists(atPath: root.path, isDirectory: &isDirectory) else { continue }
            if !isDirectory.boolValue {
                found.append((root, root.lastPathComponent, modified(root)))
                continue
            }
            let base = root.standardizedFileURL.path
            let walk = manager.enumerator(
                at: root, includingPropertiesForKeys: keys, options: [.skipsHiddenFiles, .skipsPackageDescendants])
            var inside: [(url: URL, name: String, modified: Date)] = []
            while let url = walk?.nextObject() as? URL {
                guard ["c4", "likec4", "svg"].contains(url.pathExtension) else { continue }
                let path = url.standardizedFileURL.path
                let name = path.hasPrefix(base + "/") ? String(path.dropFirst(base.count + 1)) : url.lastPathComponent
                inside.append((url, name, modified(url)))
            }
            found += inside.sorted { $0.name < $1.name }
        }
        return found
    }
}
