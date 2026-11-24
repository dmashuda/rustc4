import C4FFI
import XCTest

/// Swift -> UniFFI -> Rust. What the viewer does is tested in //core/c4_ui;
/// this checks the generated bindings carry the screen, the diagram and the
/// calls across.
final class C4FFITests: XCTestCase {
    func testTheExampleOpensOnItsLandscape() {
        let ui = C4Ui.example()
        let screen = ui.screen()
        XCTAssertEqual(screen.title, "Landscape")
        XCTAssertEqual(screen.views.map(\.id), ["view.index", "view.shop", "view.api", "view.orders", "view.checkout"])
        XCTAssertEqual(screen.exports.map(\.kind), [.svg, .png])
        XCTAssertEqual(screen.controls.open, "Open…")
        XCTAssertEqual(screen.editor?.file, 0)
        XCTAssertNil(screen.details)
    }

    func testTheDiagramCrossesWhole() throws {
        let ui = C4Ui.example()
        let diagram = try XCTUnwrap(ui.tap(id: "view.shop").diagram)
        XCTAssertEqual(diagram.id, "shop")
        XCTAssertGreaterThan(diagram.width, 400)
        let shop = try XCTUnwrap(diagram.nodes.first { $0.id == "shop" })
        XCTAssertTrue(shop.group)
        XCTAssertEqual(shop.labels.first?.role, .groupTitle)
        let web = try XCTUnwrap(diagram.nodes.first { $0.id == "shop.web" })
        XCTAssertEqual(web.shape, .browser)
        XCTAssertEqual(web.parent, "shop")
        XCTAssertEqual(web.labels.map(\.text), ["Storefront", "TypeScript, React", "What a customer sees"])
        XCTAssertEqual(web.labels.first?.anchor, .middle)
        let calls = try XCTUnwrap(diagram.edges.first { $0.id == "shop.web->shop.api" })
        XCTAssertEqual(calls.path.count % 3, 1, "a start, then three points a curve")
        XCTAssertEqual(calls.line, .dashed)
        XCTAssertEqual(calls.markers.first?.points.count, 3)
        XCTAssertEqual(diagram.nodes.first { $0.id == "shop.api" }?.navigateTo, "api")
    }

    func testTapsAndEditsReachRust() {
        let ui = C4Ui.example()
        XCTAssertEqual(ui.tap(id: "open.shop").view, "shop")
        XCTAssertEqual(ui.tap(id: "node.shop.api").details?.title, "API")
        XCTAssertEqual(ui.tap(id: "back").view, "index")

        let screen = ui.setSource(file: 0, text: "specification { element box } model { a = box; a -> b }")
        XCTAssertEqual(screen.problems.map(\.text), ["There is no element 'b'."])
        XCTAssertEqual(screen.problems.first?.error, true)
        XCTAssertEqual(screen.diagram?.nodes.map(\.id), ["a"])
    }

    func testAnotherModelAndItsSvg() {
        let ui = C4Ui(sources: [], editable: false)
        XCTAssertNil(ui.screen().diagram)
        XCTAssertEqual(ui.svg(background: nil), "")
        let source = Source(name: "m.c4", text: "specification { element box } model { a = box 'A' }")
        let screen = ui.setSources(sources: [source], editable: false)
        XCTAssertNil(screen.editor)
        XCTAssertEqual(screen.status, "1 element · 0 relationships · 1 view")
        XCTAssertTrue(ui.svg(background: "#ffffff").hasPrefix("<svg "))
    }
}
