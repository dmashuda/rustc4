import C4FFI
import SwiftUI

extension Color {
    /// `#rrggbb` or `#rgb`, as the diagram carries its colors.
    init(hex: String) {
        var digits = hex.hasPrefix("#") ? String(hex.dropFirst()) : hex
        if digits.count == 3 { digits = digits.map { "\($0)\($0)" }.joined() }
        let value = UInt32(digits, radix: 16) ?? 0x888888
        self.init(
            .sRGB,
            red: Double((value >> 16) & 0xff) / 255,
            green: Double((value >> 8) & 0xff) / 255,
            blue: Double(value & 0xff) / 255)
    }
}

/// Draws a `Diagram` exactly as given: every position, color, line break
/// and curve was decided in Rust (//core/c4_layout). The shapes here match
/// the SVG the same diagram becomes (//core/c4_render), number for number.
struct DiagramDrawing {
    let diagram: Diagram
    /// A tap id, `node.<id>` or `edge.<id>`, or empty.
    let selected: String
    /// With a selection: the ids of everything connected to it. The rest
    /// is drawn faded. Empty: nothing is.
    var related: Set<String> = []
    let accent: Color
    /// The images of the diagram's icons that are to hand, by `href`.
    var icons: [String: Image] = [:]

    /// A context to draw the thing with this id in: faded if it is not
    /// part of what is selected.
    private func context(for id: String, from context: GraphicsContext) -> GraphicsContext {
        var copy = context
        if !related.isEmpty, !related.contains(id) { copy.opacity = 0.18 }
        return copy
    }

    func draw(in context: inout GraphicsContext) {
        // A sequence's lines down, behind everything.
        for line in diagram.lifelines {
            var path = Path()
            path.move(to: CGPoint(x: line.from.x, y: line.from.y))
            path.addLine(to: CGPoint(x: line.to.x, y: line.to.y))
            context.stroke(path, with: .color(Color(hex: line.color)), style: stroke(line.line, width: 1.2))
        }
        for node in diagram.nodes where node.group {
            var faded = self.context(for: "node.\(node.id)", from: context)
            draw(node, in: &faded)
        }
        for edge in diagram.edges {
            var faded = self.context(for: "edge.\(edge.id)", from: context)
            draw(edge, in: &faded)
        }
        for node in diagram.nodes where !node.group {
            var faded = self.context(for: "node.\(node.id)", from: context)
            draw(node, in: &faded)
        }
        for edge in diagram.edges {
            var faded = self.context(for: "edge.\(edge.id)", from: context)
            let on = selected == "edge.\(edge.id)"
            for label in edge.labels { draw(label, in: &faded, color: on ? accent : nil) }
        }
    }

    private func stroke(_ line: LineStyle, width: CGFloat) -> StrokeStyle {
        switch line {
        case .solid: StrokeStyle(lineWidth: width, lineJoin: .round)
        case .dashed: StrokeStyle(lineWidth: width, lineJoin: .round, dash: [6, 4])
        case .dotted: StrokeStyle(lineWidth: width, lineCap: .round, dash: [1.5, 4])
        }
    }

    // MARK: Nodes

    private func draw(_ node: Node, in context: inout GraphicsContext) {
        let rect = CGRect(x: node.x, y: node.y, width: node.width, height: node.height)
        let on = selected == "node.\(node.id)"
        let fill = Color(hex: node.fill)
        let edge = on ? accent : Color(hex: node.stroke)
        let line = on ? StrokeStyle(lineWidth: 3.5) : stroke(node.border, width: node.group ? 1.25 : 1.5)

        if node.group {
            let outline = Path(roundedRect: rect, cornerRadius: 8)
            context.fill(outline, with: .color(fill.opacity(node.fillOpacity)))
            context.stroke(outline, with: .color(edge.opacity(on ? 1 : 0.55)), style: line)
        } else {
            // Each shape: what to fill with the node's color and outline.
            func filled(_ path: Path) {
                context.fill(path, with: .color(fill))
                context.stroke(path, with: .color(edge), style: line)
            }
            func shape(in rect: CGRect) {
                let (x, y, w, h) = (rect.minX, rect.minY, rect.width, rect.height)
                switch node.shape {
                case .rectangle:
                    filled(Path(roundedRect: rect, cornerRadius: 6))
                case .person:
                    // Shoulders, then a head overlapping them.
                    filled(Path(roundedRect: CGRect(x: x, y: y + 24, width: w, height: h - 24), cornerRadius: 18))
                    filled(Path(ellipseIn: CGRect(x: rect.midX - 19, y: y + 0.75, width: 38, height: 38)))
                case .browser:
                    let frame = Path(roundedRect: rect, cornerRadius: 6)
                    context.fill(frame, with: .color(Color(hex: node.stroke)))
                    context.stroke(frame, with: .color(edge), style: line)
                    let page = CGRect(x: x + 4, y: y + 22, width: w - 8, height: h - 26)
                    context.fill(Path(roundedRect: page, cornerRadius: 3), with: .color(fill))
                    for i in 0..<3 {
                        let dot = CGRect(x: x + 9.5 + 12 * CGFloat(i), y: y + 7.5, width: 7, height: 7)
                        context.fill(Path(ellipseIn: dot), with: .color(.white.opacity(0.55)))
                    }
                case .mobile:
                    let frame = Path(roundedRect: rect, cornerRadius: 10)
                    context.fill(frame, with: .color(Color(hex: node.stroke)))
                    context.stroke(frame, with: .color(edge), style: line)
                    let display = CGRect(x: x + 10, y: y + 5, width: w - 32, height: h - 10)
                    context.fill(Path(roundedRect: display, cornerRadius: 4), with: .color(fill))
                    let button = CGRect(x: rect.maxX - 16, y: rect.midY - 5, width: 10, height: 10)
                    context.fill(Path(ellipseIn: button), with: .color(fill))
                case .cylinder, .storage:
                    // The base, the sides, then the lid on top.
                    let ry: CGFloat = 10
                    filled(Path(ellipseIn: CGRect(x: x, y: rect.maxY - 2 * ry, width: w, height: 2 * ry)))
                    context.fill(Path(CGRect(x: x, y: y + ry, width: w, height: h - 2 * ry)), with: .color(fill))
                    var sides = Path()
                    sides.move(to: CGPoint(x: x, y: y + ry))
                    sides.addLine(to: CGPoint(x: x, y: rect.maxY - ry))
                    sides.move(to: CGPoint(x: rect.maxX, y: y + ry))
                    sides.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY - ry))
                    context.stroke(sides, with: .color(edge), style: line)
                    filled(Path(ellipseIn: CGRect(x: x, y: y, width: w, height: 2 * ry)))
                case .queue:
                    // The same, on its side: the open end is on the right.
                    let rx: CGFloat = 10
                    filled(Path(ellipseIn: CGRect(x: x, y: y, width: 2 * rx, height: h)))
                    context.fill(Path(CGRect(x: x + rx, y: y, width: w - 2 * rx, height: h)), with: .color(fill))
                    var sides = Path()
                    sides.move(to: CGPoint(x: x + rx, y: y))
                    sides.addLine(to: CGPoint(x: rect.maxX - rx, y: y))
                    sides.move(to: CGPoint(x: x + rx, y: rect.maxY))
                    sides.addLine(to: CGPoint(x: rect.maxX - rx, y: rect.maxY))
                    context.stroke(sides, with: .color(edge), style: line)
                    filled(Path(ellipseIn: CGRect(x: rect.maxX - 2 * rx, y: y, width: 2 * rx, height: h)))
                }
            }
            // A stack: the ones behind first.
            for corner in node.behind {
                shape(in: CGRect(x: corner.x, y: corner.y, width: node.width, height: node.height))
            }
            shape(in: rect)
        }
        if let icon = node.icon, let image = icons[icon.href] {
            // Whole and undistorted, in the middle of where it goes.
            let resolved = context.resolve(image)
            let (w, h) = (resolved.size.width, resolved.size.height)
            if w > 0, h > 0 {
                let scale = min(icon.width / w, icon.height / h)
                let fitted = CGRect(
                    x: icon.x + (icon.width - w * scale) / 2, y: icon.y + (icon.height - h * scale) / 2,
                    width: w * scale, height: h * scale)
                context.draw(resolved, in: fitted)
            }
        }
        for label in node.labels { draw(label, in: &context, color: nil) }
        if node.navigateTo != nil {
            // A magnifying glass in the corner: there is a closer view.
            let center = DiagramHit.glass(of: node)
            var glass = Path(ellipseIn: CGRect(x: center.x - 4.5, y: center.y - 4.5, width: 9, height: 9))
            glass.move(to: CGPoint(x: center.x + 3.4, y: center.y + 3.4))
            glass.addLine(to: CGPoint(x: center.x + 7.4, y: center.y + 7.4))
            let color = node.group ? Color(hex: node.stroke) : .white
            context.stroke(glass, with: .color(color.opacity(0.85)), style: StrokeStyle(lineWidth: 1.6, lineCap: .round))
        }
    }

    // MARK: Edges

    private func draw(_ edge: C4FFI.Edge, in context: inout GraphicsContext) {
        let on = selected == "edge.\(edge.id)"
        let color = on ? accent : Color(hex: edge.color)
        context.stroke(DiagramHit.path(of: edge), with: .color(color), style: stroke(edge.line, width: on ? 2.8 : 1.6))
        for marker in edge.markers {
            var outline = Path()
            if marker.radius > 0, let center = marker.points.first {
                outline.addEllipse(
                    in: CGRect(
                        x: center.x - marker.radius, y: center.y - marker.radius,
                        width: 2 * marker.radius, height: 2 * marker.radius))
            } else {
                outline.addLines(marker.points.map { CGPoint(x: $0.x, y: $0.y) })
                if marker.closed { outline.closeSubpath() }
            }
            if marker.filled { context.fill(outline, with: .color(color)) }
            context.stroke(outline, with: .color(color), style: StrokeStyle(lineWidth: 1.6, lineCap: .round, lineJoin: .round))
        }
    }

    // MARK: Text

    /// One line, on its baseline. Never wrapped or measured for layout:
    /// Rust did both, for a font of this family.
    private func draw(_ label: C4FFI.Label, in context: inout GraphicsContext, color: Color?) {
        let font = Font.custom("Helvetica", fixedSize: label.size).weight(label.bold ? .bold : .regular)
        let text = context.resolve(Text(label.text).font(font).foregroundColor(color ?? Color(hex: label.color)))
        let size = text.measure(in: CGSize(width: CGFloat.infinity, height: .infinity))
        let x = label.anchor == .middle ? label.x - size.width / 2 : label.x
        let top = label.y - text.firstBaseline(in: size)
        context.draw(text, in: CGRect(x: x, y: top, width: size.width + 1, height: size.height))
    }
}

/// What is where in a diagram: which thing a point is on.
enum DiagramHit {
    static func path(of edge: C4FFI.Edge) -> Path {
        var path = Path()
        guard let start = edge.path.first else { return path }
        path.move(to: CGPoint(x: start.x, y: start.y))
        var i = 1
        while i + 2 < edge.path.count {
            let (a, b, c) = (edge.path[i], edge.path[i + 1], edge.path[i + 2])
            path.addCurve(
                to: CGPoint(x: c.x, y: c.y),
                control1: CGPoint(x: a.x, y: a.y),
                control2: CGPoint(x: b.x, y: b.y))
            i += 3
        }
        return path
    }

    /// The middle of the magnifying glass drawn on a node with a closer view.
    static func glass(of node: Node) -> CGPoint {
        CGPoint(x: node.x + node.width - 17, y: node.y + (node.group ? 22 : 16))
    }

    /// The id to tap for a point in the diagram's own coordinates.
    /// `opening`: a double click, which goes to the closer view of a node
    /// that has one; a click on the glass does that by itself.
    static func id(at point: CGPoint, in diagram: Diagram, opening: Bool = false) -> String {
        let hit = { (node: Node) -> String? in
            guard CGRect(x: node.x, y: node.y, width: node.width, height: node.height).contains(point) else {
                return nil
            }
            let glass = glass(of: node)
            let onGlass = abs(point.x - glass.x) <= 12 && abs(point.y - glass.y) <= 12
            return node.navigateTo != nil && (opening || onGlass) ? "open.\(node.id)" : "node.\(node.id)"
        }
        // Topmost first: boxes, then lines, then the groups behind them
        // (innermost first: they come later in the list).
        for node in diagram.nodes.reversed() where !node.group {
            if let id = hit(node) { return id }
        }
        for edge in diagram.edges.reversed() {
            let near = path(of: edge).strokedPath(StrokeStyle(lineWidth: 14, lineCap: .round))
            if near.contains(point) { return "edge.\(edge.id)" }
        }
        for node in diagram.nodes.reversed() where node.group {
            if let id = hit(node) { return id }
        }
        return "canvas"
    }
}
