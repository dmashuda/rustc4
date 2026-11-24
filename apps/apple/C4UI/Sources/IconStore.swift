import C4FFI
import Foundation
import Observation
import SwiftUI

/// The images of a diagram's icons, by the `href` the diagram gives each.
///
/// What is drawn where was decided in Rust; getting hold of the picture
/// is the platform's: a `data:` URL holds its own bytes, anything else is
/// fetched, once, and the diagram drawn again when it arrives.
@MainActor
@Observable
final class IconStore {
    /// Goes up as each fetched image arrives.
    private(set) var arrived = 0
    @ObservationIgnored private var images: [String: Image] = [:]
    @ObservationIgnored private var asked: Set<String> = []

    /// Those of this diagram's icons that are to hand now.
    func images(for diagram: Diagram) -> [String: Image] {
        _ = arrived
        var found: [String: Image] = [:]
        for node in diagram.nodes {
            if let href = node.icon?.href, let image = image(for: href) { found[href] = image }
        }
        return found
    }

    private func image(for href: String) -> Image? {
        if let image = images[href] { return image }
        guard asked.insert(href).inserted else { return nil }
        if href.hasPrefix("data:") {
            let image = Self.bytes(ofDataURL: href).flatMap(Self.image(from:))
            images[href] = image
            return image
        }
        guard let url = URL(string: href), url.scheme == "https" || url.scheme == "http" else { return nil }
        Task {
            guard let (data, _) = try? await URLSession.shared.data(from: url),
                let image = Self.image(from: data)
            else { return }
            images[href] = image
            arrived += 1
        }
        return nil
    }

    /// What a `data:` URL holds: base64, or percent-encoded text.
    static func bytes(ofDataURL href: String) -> Data? {
        guard let comma = href.firstIndex(of: ",") else { return nil }
        let body = String(href[href.index(after: comma)...])
        if href[..<comma].hasSuffix(";base64") { return Data(base64Encoded: body) }
        return body.removingPercentEncoding?.data(using: .utf8)
    }

    static func image(from data: Data) -> Image? {
        #if canImport(AppKit)
        return NSImage(data: data).map { Image(nsImage: $0) }
        #elseif canImport(UIKit)
        return UIImage(data: data).map { Image(uiImage: $0) }
        #else
        return nil
        #endif
    }
}
