import C4FFI
import SwiftUI

/// How the diagram is being looked at. None of this reaches Rust: it
/// changes the view of the drawing, not what is drawn.
struct Camera: Equatable {
    var scale: CGFloat = 1
    var offset: CGSize = .zero
    /// Still as `fit` left it: a resized window fits again. Moving or
    /// zooming by hand ends that.
    var fitted = true

    /// The whole diagram in view, never blown up past its natural size.
    mutating func fit(_ diagram: Diagram, in size: CGSize) {
        guard diagram.width > 0, diagram.height > 0, size.width > 0, size.height > 0 else { return }
        scale = min(1, (size.width - 32) / diagram.width, (size.height - 32) / diagram.height)
        scale = max(scale, 0.05)
        offset = CGSize(
            width: (size.width - diagram.width * scale) / 2,
            height: (size.height - diagram.height * scale) / 2)
        fitted = true
    }

    mutating func zoom(by factor: CGFloat, around point: CGPoint) {
        let next = min(4, max(0.1, scale * factor))
        offset.width = point.x - (point.x - offset.width) * next / scale
        offset.height = point.y - (point.y - offset.height) * next / scale
        scale = next
        fitted = false
    }

    mutating func pan(by delta: CGSize) {
        offset.width += delta.width
        offset.height += delta.height
        fitted = false
    }

    /// A point on screen, in the diagram's coordinates.
    func diagramPoint(_ point: CGPoint) -> CGPoint {
        CGPoint(x: (point.x - offset.width) / scale, y: (point.y - offset.height) / scale)
    }
}

/// The diagram, to drag, scroll, pinch and click.
struct DiagramCanvas: View {
    let diagram: Diagram
    let selected: String
    let related: [String]
    let controls: Controls
    let onTap: (String) -> Void

    @State private var camera = Camera()
    @State private var size = CGSize.zero
    @State private var dragged = CGSize.zero

    var body: some View {
        // Read here, so the canvas is drawn again as each one arrives.
        let images = IconStore.shared.images(for: diagram)
        return GeometryReader { geometry in
            Canvas { context, _ in
                context.translateBy(x: camera.offset.width, y: camera.offset.height)
                context.scaleBy(x: camera.scale, y: camera.scale)
                DiagramDrawing(
                    diagram: diagram, selected: selected, related: Set(related), accent: .accentColor,
                    icons: images
                )
                .draw(in: &context)
            }
            .accessibilityIdentifier("diagram")
            .accessibilityLabel(diagram.title)
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 3)
                    .onChanged { value in
                        camera.pan(
                            by: CGSize(
                                width: value.translation.width - dragged.width,
                                height: value.translation.height - dragged.height))
                        dragged = value.translation
                    }
                    .onEnded { _ in dragged = .zero }
            )
            .simultaneousGesture(
                SpatialTapGesture(count: 2).onEnded { value in
                    let id = DiagramHit.id(at: camera.diagramPoint(value.location), in: diagram, opening: true)
                    if id.hasPrefix("open.") { onTap(id) }
                }
            )
            .simultaneousGesture(
                SpatialTapGesture().onEnded { value in
                    onTap(DiagramHit.id(at: camera.diagramPoint(value.location), in: diagram))
                }
            )
            #if os(macOS)
            .background(
                ScrollAndPinch { delta, location, zooming in
                    if zooming {
                        camera.zoom(by: delta.height, around: location)
                    } else {
                        camera.pan(by: delta)
                    }
                }
            )
            #endif
            .onAppear {
                size = geometry.size
                camera.fit(diagram, in: size)
            }
            .onChange(of: geometry.size) { _, new in
                size = new
                if camera.fitted { camera.fit(diagram, in: new) }
            }
            // Another view starts fitted; an edit to this one keeps the
            // place it is being looked at from.
            .onChange(of: diagram.id) { camera.fit(diagram, in: size) }
        }
        .overlay(alignment: .bottomTrailing) { zoomButtons }
    }

    private var zoomButtons: some View {
        let center = CGPoint(x: size.width / 2, y: size.height / 2)
        return HStack(spacing: 0) {
            zoomButton("minus", controls.zoomOut) { camera.zoom(by: 1 / 1.25, around: center) }
            Divider().frame(height: 16)
            zoomButton("plus", controls.zoomIn) { camera.zoom(by: 1.25, around: center) }
            Divider().frame(height: 16)
            zoomButton("arrow.up.left.and.arrow.down.right", controls.fit) { camera.fit(diagram, in: size) }
        }
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8))
        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(.separator))
        .padding(12)
    }

    private func zoomButton(_ symbol: String, _ label: String, _ action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol).frame(width: 30, height: 28).contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(label)
        .accessibilityLabel(label)
    }
}

#if os(macOS)
/// Scrolling and pinching over the diagram, which SwiftUI has no gesture
/// for: two fingers move it, a pinch (or scrolling with ⌘ held) zooms
/// around the pointer. Reports the movement, or for a zoom the factor (as
/// `delta.height`), with where the pointer is.
private struct ScrollAndPinch: NSViewRepresentable {
    let onEvent: (_ delta: CGSize, _ location: CGPoint, _ zooming: Bool) -> Void

    func makeNSView(context: Context) -> Catcher {
        let view = Catcher()
        view.onEvent = onEvent
        return view
    }

    func updateNSView(_ view: Catcher, context: Context) {
        view.onEvent = onEvent
    }

    final class Catcher: NSView {
        var onEvent: ((CGSize, CGPoint, Bool) -> Void)?
        private var monitor: Any?

        // Top-left origin, like SwiftUI.
        override var isFlipped: Bool { true }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            if let monitor { NSEvent.removeMonitor(monitor) }
            monitor = nil
            guard window != nil else { return }
            // The view itself sits behind the canvas and is sent nothing,
            // so the events are picked out of the window's as they pass.
            monitor = NSEvent.addLocalMonitorForEvents(matching: [.scrollWheel, .magnify]) { [weak self] event in
                guard let self, event.window === self.window else { return event }
                let location = self.convert(event.locationInWindow, from: nil)
                guard self.bounds.contains(location) else { return event }
                if event.type == .magnify {
                    self.onEvent?(CGSize(width: 0, height: 1 + event.magnification), location, true)
                } else if event.modifierFlags.contains(.command) {
                    self.onEvent?(CGSize(width: 0, height: exp(event.scrollingDeltaY * 0.01)), location, true)
                } else {
                    // A mouse wheel reports lines, a trackpad points.
                    let step: CGFloat = event.hasPreciseScrollingDeltas ? 1 : 10
                    let delta = CGSize(width: event.scrollingDeltaX * step, height: event.scrollingDeltaY * step)
                    self.onEvent?(delta, location, false)
                }
                return nil
            }
        }

        deinit {
            if let monitor { NSEvent.removeMonitor(monitor) }
        }
    }
}
#endif
