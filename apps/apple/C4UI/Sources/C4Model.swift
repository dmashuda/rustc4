import C4FFI
import Foundation
import Observation

/// Holds the Rust `C4Ui` and its latest `Screen` for SwiftUI.
///
/// Nothing here knows what is on the screen: views draw `screen` and call
/// `tap` with what the user did. What is here is what only the platform can
/// do: reading a model's files from disk, noticing when they change, and
/// remembering which they were. The files are written in an editor; this
/// only shows them.
@MainActor
@Observable
public final class C4Model {
    public private(set) var screen: Screen
    /// What was opened: files and directories, re-read when they change.
    /// Empty until something is.
    public private(set) var roots: [URL] = []

    private let ui: C4Ui
    /// Where what was opened is kept for the next launch, if anywhere.
    @ObservationIgnored private let defaults: UserDefaults?
    private static let rememberedKey = "openedPaths"
    /// How often the files are looked at: as `rustc4 serve` does.
    private static let poll: TimeInterval = 0.25
    @ObservationIgnored private var fingerprint = ""
    /// What the model named besides its own files, when it was last read:
    /// an icon's image, a description's text. Watched like the rest.
    @ObservationIgnored private var named: [URL] = []
    @ObservationIgnored private var timer: Timer?

    /// Opens on nothing, saying so, until `open` or `reopen`.
    public init(ui: C4Ui = C4Ui(sources: [], editable: false), remembering defaults: UserDefaults? = nil) {
        self.ui = ui
        self.defaults = defaults
        screen = ui.screen()
    }

    // MARK: What the user did

    public func tap(_ id: String) {
        screen = ui.tap(id: id)
    }

    /// The showing diagram as SVG text, for saving.
    public func svg(background: String? = nil) -> String {
        ui.svg(background: background)
    }

    // MARK: Models on disk

    /// The files and folders a command line names. `bazel run` starts the
    /// app somewhere of its own and says where the command was typed, in
    /// `BUILD_WORKING_DIRECTORY`: a path that is not absolute is meant
    /// from there.
    public nonisolated static func urls(forArguments arguments: [String], environment: [String: String]) -> [URL] {
        let typedIn = environment["BUILD_WORKING_DIRECTORY"].map { URL(fileURLWithPath: $0, isDirectory: true) }
        return arguments.filter { !$0.hasPrefix("-") }.map { path in
            URL(fileURLWithPath: path, relativeTo: path.hasPrefix("/") ? nil : typedIn).standardizedFileURL
        }
    }

    /// Shows the model in these files and directories, and keeps showing
    /// it as they change: edit in any editor, and the window follows.
    public func open(_ urls: [URL], watching: Bool = true) {
        roots = urls
        defaults?.set(urls.map(\.path), forKey: Self.rememberedKey)
        fingerprint = ""
        reloadIfChanged()
        timer?.invalidate()
        guard watching else { return }
        timer = Timer.scheduledTimer(withTimeInterval: Self.poll, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.reloadIfChanged() }
        }
    }

    /// Opens what was open when the app was last used, if it is still there.
    public func reopen(watching: Bool = true) {
        let urls = (defaults?.stringArray(forKey: Self.rememberedKey) ?? [])
            .filter { FileManager.default.fileExists(atPath: $0) }
            .map { URL(fileURLWithPath: $0) }
        if !urls.isEmpty { open(urls, watching: watching) }
    }

    /// Re-reads the opened files if any was added, removed or saved; says
    /// whether it did.
    @discardableResult
    public func reloadIfChanged() -> Bool {
        guard !roots.isEmpty else { return false }
        let files = Self.sourceFiles(in: roots)
        let stamp = { (urls: [URL]) in
            urls.map { url in
                // Asked of the file each time: a URL remembers what it was told.
                let attributes = try? FileManager.default.attributesOfItem(atPath: url.path)
                let modified = attributes?[.modificationDate] as? Date
                let size = attributes?[.size] as? UInt64
                return "\(url.path)@\((modified ?? .distantPast).timeIntervalSince1970)+\(size ?? 0)"
            }.joined(separator: "\n")
        }
        guard stamp(files.map(\.url) + named) != fingerprint else { return false }
        var sources = files.compactMap { file in
            (try? String(contentsOf: file.url, encoding: .utf8)).map { Source(name: file.name, text: $0) }
        }
        screen = ui.setSources(sources: sources, editable: false)
        // What the model names besides: Rust says which files, by the
        // names they would have; finding and reading them is done here.
        named = []
        for name in ui.wanted() {
            guard let url = Self.locate(name, in: roots), let text = try? String(contentsOf: url, encoding: .utf8)
            else { continue }
            sources.append(Source(name: name, text: text))
            named.append(url)
        }
        if !named.isEmpty { screen = ui.setSources(sources: sources, editable: false) }
        fingerprint = stamp(files.map(\.url) + named)
        return true
    }

    /// The file a source means by this name: names are paths below a root
    /// that is a folder, or beside one that is a file.
    static func locate(_ name: String, in roots: [URL]) -> URL? {
        for root in roots {
            var isDirectory: ObjCBool = false
            guard FileManager.default.fileExists(atPath: root.path, isDirectory: &isDirectory) else { continue }
            let base = isDirectory.boolValue ? root : root.deletingLastPathComponent()
            let url = base.appendingPathComponent(name).standardizedFileURL
            if FileManager.default.fileExists(atPath: url.path) { return url }
        }
        return nil
    }

    /// Every `.c4` and `.likec4` file under the roots, in a fixed order,
    /// named by its path below the root it was found under.
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
                // As the command line does: nobody keeps a model in there,
                // and it is a great deal to look through four times a second.
                if url.lastPathComponent == "node_modules" {
                    walk?.skipDescendants()
                    continue
                }
                guard ["c4", "likec4"].contains(url.pathExtension) else { continue }
                let path = url.standardizedFileURL.path
                let name = path.hasPrefix(base + "/") ? String(path.dropFirst(base.count + 1)) : url.lastPathComponent
                inside.append((url, name, modified(url)))
            }
            found += inside.sorted { $0.name < $1.name }
        }
        return found
    }
}
