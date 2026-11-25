import C4FFI
import SwiftUI
import XCTest

@testable import C4UI

/// What is the platform's own: reading models from disk and following
/// them, finding what is under a click, looking at the diagram, and making
/// an image. What the viewer does is tested in //core/c4_ui.
@MainActor
final class C4UITests: XCTestCase {
    private func temporaryDirectory() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("c4ui-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        return url
    }

    /// The example model: something to click on and draw.
    private func example() -> C4Model { C4Model(ui: .example()) }

    func testOpensOnNothingAndSaysSo() {
        let model = C4Model()
        XCTAssertNil(model.screen.diagram)
        XCTAssertNil(model.screen.editor, "the source is written elsewhere")
        XCTAssertTrue(model.screen.empty.hasPrefix("No model is open."), model.screen.empty)
        XCTAssertTrue(model.roots.isEmpty)
        XCTAssertFalse(model.svg().hasPrefix("<svg"))
        XCTAssertFalse(model.reloadIfChanged())
    }

    func testWhatWasOpenIsOpenedAgain() throws {
        let suite = "c4ui-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        addTeardownBlock { defaults.removePersistentDomain(forName: suite) }
        let folder = try temporaryDirectory()
        try "specification { element box } model { a = box 'A' }"
            .write(to: folder.appendingPathComponent("m.c4"), atomically: true, encoding: .utf8)

        // Nothing remembered: nothing opened.
        let first = C4Model(remembering: defaults)
        first.reopen(watching: false)
        XCTAssertTrue(first.roots.isEmpty)
        first.open([folder], watching: false)

        let next = C4Model(remembering: defaults)
        next.reopen(watching: false)
        XCTAssertEqual(next.roots.map(\.path), [folder.path])
        XCTAssertEqual(next.screen.status, "1 element · 0 relationships · 1 view")

        // Gone since: back to nothing, not to an error.
        try FileManager.default.removeItem(at: folder)
        let later = C4Model(remembering: defaults)
        later.reopen(watching: false)
        XCTAssertTrue(later.roots.isEmpty)
    }

    func testFilesTheModelNamesAreReadWithIt() throws {
        let folder = try temporaryDirectory()
        try FileManager.default.createDirectory(at: folder.appendingPathComponent("docs"), withIntermediateDirectories: true)
        try "specification { element box } model { a = box 'A' { descriptionFile ./docs/a.md; icon ./a.svg } }"
            .write(to: folder.appendingPathComponent("m.c4"), atomically: true, encoding: .utf8)
        let text = folder.appendingPathComponent("docs/a.md")
        try "What A does.".write(to: text, atomically: true, encoding: .utf8)
        try "<svg xmlns='http://www.w3.org/2000/svg'/>"
            .write(to: folder.appendingPathComponent("a.svg"), atomically: true, encoding: .utf8)
        try "Nobody names this.".write(to: folder.appendingPathComponent("docs/other.md"), atomically: true, encoding: .utf8)

        let model = C4Model()
        model.open([folder], watching: false)
        XCTAssertEqual(model.screen.problems.map(\.text), [])
        XCTAssertEqual(model.screen.diagram?.nodes.first?.icon?.href.hasPrefix("data:image/svg+xml;base64,"), true)
        model.tap("node.a")
        XCTAssertEqual(model.screen.details?.description, "What A does.")

        // A file the model names is followed like the model's own.
        try "What A does now.".write(to: text, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.modificationDate: Date().addingTimeInterval(5)], ofItemAtPath: text.path)
        model.reloadIfChanged()
        XCTAssertEqual(model.screen.details?.description, "What A does now.")
    }

    func testPathsOnTheCommandLineAreMeantFromWhereItWasTyped() {
        // As `bazel run //apps/macos:rustc4 -- examples/library` gives them.
        let urls = C4Model.urls(
            forArguments: ["examples/library", "-NSDocumentRevisionsDebugMode", "/tmp/other.c4", "../up"],
            environment: ["BUILD_WORKING_DIRECTORY": "/Users/me/rustc4"])
        XCTAssertEqual(urls.map(\.path), ["/Users/me/rustc4/examples/library", "/tmp/other.c4", "/Users/me/up"])
        // Started any other way, from wherever it was started.
        let here = FileManager.default.currentDirectoryPath
        XCTAssertEqual(C4Model.urls(forArguments: ["a.c4"], environment: [:]).map(\.path), [here + "/a.c4"])
    }

    /// Everything a diagram can hold, drawn by this renderer: a picture of
    /// it is left with the test's outputs, to compare with the same model
    /// from `rustc4 export png`.
    func testEveryKindOfThingDraws() throws {
        let folder = try temporaryDirectory()
        try """
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="10" fill="#fde047"/></svg>
        """.write(to: folder.appendingPathComponent("dot.svg"), atomically: true, encoding: .utf8)
        try """
        specification {
          element person { notation 'A person' style { shape person; color green } }
          element service { notation 'A service' style { icon ./dot.svg } }
          element store { notation 'A store' style { shape storage; icon ./dot.svg; iconPosition top } }
          element note { style { border none; color muted } }
          relationship has { notation 'One has many' line solid; head crow }
          relationship feed { notation 'Overnight' line dotted; head vee }
        }
        model {
          user = person 'User'
          app = service 'App' 'Does the work' 'Rust' { style { multiple true } }
          big = service 'Big' 'In bigger type' { style { size large; textSize large } }
          db = store 'Database' 'Keeps it'
          aside = note 'A note' 'With no border'
          user -> app 'uses'
          app -[has]-> db 'rows'
          app -[feed]-> big 'reports'
          app <-> user 'talks back'
        }
        views { view index { include * } }
        """.write(to: folder.appendingPathComponent("m.c4"), atomically: true, encoding: .utf8)

        let model = C4Model()
        model.open([folder], watching: false)
        XCTAssertEqual(model.screen.problems.map(\.text), [])
        let diagram = try XCTUnwrap(model.screen.diagram)
        XCTAssertNotNil(diagram.legend)
        let data = try XCTUnwrap(C4View.png(of: diagram, scale: 2))
        if let outputs = ProcessInfo.processInfo.environment["TEST_UNDECLARED_OUTPUTS_DIR"] {
            try data.write(to: URL(fileURLWithPath: outputs).appendingPathComponent("everything.png"))
            try model.svg(background: "#ffffff").write(
                to: URL(fileURLWithPath: outputs).appendingPathComponent("everything.svg"), atomically: true, encoding: .utf8)
        }
        let image = try XCTUnwrap(NSBitmapImageRep(data: data))
        let color = { (x: Double, y: Double) in
            image.colorAt(x: Int(x * 2), y: Int(y * 2))?.usingColorSpace(.sRGB)
        }
        // The icon is drawn: yellow in the middle of where it goes.
        let icon = try XCTUnwrap(diagram.nodes.first { $0.id == "app" }?.icon)
        let middle = try XCTUnwrap(color(icon.x + icon.width / 2, icon.y + icon.height / 2))
        XCTAssertGreaterThan(middle.redComponent, 0.8)
        XCTAssertLessThan(middle.blueComponent, 0.5)
        // The one behind a stack shows past the corner of the one in front.
        let app = try XCTUnwrap(diagram.nodes.first { $0.id == "app" })
        let behind = try XCTUnwrap(color(app.x + app.width + 4, app.y + app.height + 4))
        XCTAssertGreaterThan(behind.alphaComponent, 0.9)
        // A box with no border is its fill right to the edge; one with a
        // border is darker there.
        let note = try XCTUnwrap(diagram.nodes.first { $0.id == "aside" })
        let edge = try XCTUnwrap(color(note.x + note.width / 2, note.y + 0.5))
        let inside = try XCTUnwrap(color(note.x + note.width / 2, note.y + 6))
        XCTAssertEqual(edge.blueComponent, inside.blueComponent, accuracy: 0.05)
        // And the legend's frame is there, under the drawing.
        let legend = try XCTUnwrap(diagram.legend)
        let sample = try XCTUnwrap(legend.nodes.first)
        let inSample = try XCTUnwrap(color(sample.x + sample.width / 2, sample.y + sample.height - 6))
        XCTAssertGreaterThan(inSample.alphaComponent, 0.9)
    }

    func testAFolderIsReadAndFollowed() throws {
        let folder = try temporaryDirectory()
        try FileManager.default.createDirectory(at: folder.appendingPathComponent("sub"), withIntermediateDirectories: true)
        try "specification { element box }".write(to: folder.appendingPathComponent("spec.c4"), atomically: true, encoding: .utf8)
        let modelFile = folder.appendingPathComponent("sub/model.likec4")
        try "model { a = box 'A' }".write(to: modelFile, atomically: true, encoding: .utf8)
        try "not a model".write(to: folder.appendingPathComponent("notes.txt"), atomically: true, encoding: .utf8)
        try FileManager.default.createDirectory(
            at: folder.appendingPathComponent("node_modules/dep"), withIntermediateDirectories: true)
        try "model {".write(to: folder.appendingPathComponent("node_modules/dep/theirs.c4"), atomically: true, encoding: .utf8)

        XCTAssertEqual(C4Model.sourceFiles(in: [folder]).map(\.name), ["spec.c4", "sub/model.likec4"])

        let model = C4Model()
        model.open([folder], watching: false)
        XCTAssertEqual(model.screen.status, "1 element · 0 relationships · 1 view")
        XCTAssertNil(model.screen.editor, "a model on disk is edited in an editor")

        // Nothing changed: nothing is read again.
        XCTAssertFalse(model.reloadIfChanged())

        // Saved with something new in it.
        try "model { a = box 'A'; b = box 'B'; a -> b }".write(to: modelFile, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.modificationDate: Date().addingTimeInterval(5)], ofItemAtPath: modelFile.path)
        XCTAssertTrue(model.reloadIfChanged())
        XCTAssertEqual(model.screen.status, "2 elements · 1 relationship · 1 view")

        // A single file works too.
        model.open([modelFile], watching: false)
        XCTAssertEqual(model.screen.problems.first?.file, "model.likec4")
    }

    func testAClickFindsWhatIsUnderIt() throws {
        let model = example()
        model.tap("view.shop")
        let diagram = try XCTUnwrap(model.screen.diagram)
        let node = { (id: String) in diagram.nodes.first { $0.id == id }! }
        let middle = { (n: Node) in CGPoint(x: n.x + n.width / 2, y: n.y + n.height / 2) }

        let api = node("shop.api")
        XCTAssertEqual(DiagramHit.id(at: middle(api), in: diagram), "node.shop.api")
        XCTAssertEqual(DiagramHit.id(at: middle(api), in: diagram, opening: true), "open.shop.api")
        XCTAssertEqual(DiagramHit.id(at: DiagramHit.glass(of: api), in: diagram), "open.shop.api")
        // Nothing closer to go to: a double click is a click.
        let db = node("shop.db")
        XCTAssertEqual(DiagramHit.id(at: middle(db), in: diagram, opening: true), "node.shop.db")
        // Inside the group but on none of its boxes: the group.
        let shop = node("shop")
        XCTAssertEqual(DiagramHit.id(at: CGPoint(x: shop.x + 6, y: shop.y + 6), in: diagram), "node.shop")
        XCTAssertEqual(DiagramHit.id(at: CGPoint(x: 1, y: 1), in: diagram), "canvas")
        // On a line: the end of its curve, a little before the arrowhead.
        let edge = try XCTUnwrap(diagram.edges.first { $0.id == "shop.web->shop.api" })
        let onLine = edge.path[edge.path.count / 2 / 3 * 3]
        XCTAssertEqual(DiagramHit.id(at: CGPoint(x: onLine.x, y: onLine.y), in: diagram), "edge.shop.web->shop.api")
    }

    func testTheCameraFitsAndZoomsAroundAPoint() throws {
        let diagram = try XCTUnwrap(example().screen.diagram)
        var camera = Camera()
        camera.fit(diagram, in: CGSize(width: diagram.width / 2 + 32, height: 4000))
        XCTAssertEqual(camera.scale, 0.5, accuracy: 0.001)
        XCTAssertEqual(camera.offset.width, 16, accuracy: 0.001)
        XCTAssertTrue(camera.fitted)
        // Room to spare: natural size, centered.
        camera.fit(diagram, in: CGSize(width: diagram.width + 232, height: diagram.height + 132))
        XCTAssertEqual(camera.scale, 1)
        XCTAssertEqual(camera.offset, CGSize(width: 116, height: 66))

        // What is under the pointer stays under it.
        let pointer = CGPoint(x: 300, y: 200)
        let before = camera.diagramPoint(pointer)
        camera.zoom(by: 2, around: pointer)
        let after = camera.diagramPoint(pointer)
        XCTAssertEqual(camera.scale, 2)
        XCTAssertEqual(before.x, after.x, accuracy: 0.001)
        XCTAssertEqual(before.y, after.y, accuracy: 0.001)
        XCTAssertFalse(camera.fitted)
        camera.zoom(by: 100, around: pointer)
        XCTAssertEqual(camera.scale, 4, "no closer than this")
    }

    func testTheDiagramBecomesAPng() throws {
        let diagram = try XCTUnwrap(example().screen.diagram)
        let data = try XCTUnwrap(C4View.png(of: diagram, scale: 1))
        XCTAssertEqual(Array(data.prefix(4)), [0x89, 0x50, 0x4e, 0x47])
        let image = try XCTUnwrap(NSBitmapImageRep(data: data))
        XCTAssertEqual(image.pixelsWide, Int(diagram.width))
        XCTAssertEqual(image.pixelsHigh, Int(diagram.height))
        // Something was drawn: the middle of the first box is its color.
        let box = try XCTUnwrap(diagram.nodes.first { !$0.group && $0.shape == .rectangle })
        let pixel = try XCTUnwrap(image.colorAt(x: Int(box.x + 8), y: Int(box.y + box.height / 2))?.usingColorSpace(.sRGB))
        let expected = try XCTUnwrap(NSColor(Color(hex: box.fill)).usingColorSpace(.sRGB))
        // Near enough: the image's color space is the renderer's choice.
        XCTAssertEqual(pixel.redComponent, expected.redComponent, accuracy: 0.1)
        XCTAssertEqual(pixel.greenComponent, expected.greenComponent, accuracy: 0.1)
        XCTAssertEqual(pixel.blueComponent, expected.blueComponent, accuracy: 0.1)
        XCTAssertEqual(pixel.alphaComponent, 1, accuracy: 0.01)
    }

    /// The real window's content, drawn offscreen. A picture of it is left
    /// with the test's outputs (bazel-testlogs/.../test.outputs), to look at.
    private func picture(of model: C4Model, named name: String) throws {
        let host = NSHostingView(rootView: C4View(model: model))
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1400, height: 860),
            styleMask: [.titled, .resizable, .fullSizeContentView], backing: .buffered, defer: false)
        // Closed by the teardown below, and released by ARC, not by closing.
        window.isReleasedWhenClosed = false
        window.contentView = host
        window.orderFrontRegardless()
        addTeardownBlock { window.close() }
        // A few turns of the run loop: layout, then the canvas fitting itself.
        for _ in 0..<5 {
            RunLoop.main.run(until: Date().addingTimeInterval(0.2))
            host.layoutSubtreeIfNeeded()
        }
        let image = try XCTUnwrap(host.bitmapImageRepForCachingDisplay(in: host.bounds))
        host.cacheDisplay(in: host.bounds, to: image)
        XCTAssertEqual(image.pixelsWide > 0, true)
        if let outputs = ProcessInfo.processInfo.environment["TEST_UNDECLARED_OUTPUTS_DIR"],
            let png = image.representation(using: .png, properties: [:])
        {
            try png.write(to: URL(fileURLWithPath: outputs).appendingPathComponent(name))
        }
    }

    func testTheWindowDraws() throws {
        let model = example()
        model.tap("view.shop")
        model.tap("node.shop.api")
        try picture(of: model, named: "window.png")
        // The panel says what was selected, in the screen's words.
        XCTAssertEqual(model.screen.details?.title, "API")
        // And with nothing open: the words for that, and the way to open.
        try picture(of: C4Model(), named: "window-empty.png")
    }
}
