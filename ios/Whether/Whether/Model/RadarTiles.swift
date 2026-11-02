import CoreGraphics
import Foundation
import ImageIO
import MapKit
import UniformTypeIdentifiers

/// NEXRAD tiles repainted from a radar ramp (color vision): each tile is
/// fetched as `MKTileOverlay` would, its n0q colors mapped back to their dBZ step
/// (`RadarPalette`) and painted with the ramp's color for that step. Repainted tiles
/// are kept in memory per ramp and URL while they are fresh (`RadarTileCache`), so
/// stepping through the loop again is instant but a later visit fetches new echoes.
nonisolated final class RecoloredTileOverlay: MKTileOverlay {
    /// The five legend colors, light to heavy.
    let ramp: [RGBA]
    private let rampKey: String

    /// Repainted tiles by ramp and URL, while fresh.
    private static let cache = RadarTileCache()

    init(urlTemplate: String?, ramp: [RGBA]) {
        self.ramp = ramp
        rampKey = RadarPalette.key(ramp)
        super.init(urlTemplate: urlTemplate)
    }

    override func loadTile(at path: MKTileOverlayPath, result: @escaping (Data?, (any Error)?) -> Void) {
        let url = url(forTilePath: path)
        let key = "\(rampKey)|\(url.absoluteString)"
        if let hit = Self.cache.tile(for: key, now: .now) {
            result(hit, nil)
            return
        }
        let reply = TileReply(send: result)
        let ramp = ramp
        URLSession.shared.dataTask(with: url) { data, response, error in
            let http = response as? HTTPURLResponse
            guard let data, error == nil, RadarTileCache.succeeded(status: http?.statusCode) else {
                reply.send(nil, error ?? URLError(.badServerResponse))
                return
            }
            // An error page or anything else that isn't an image is no tile: nothing
            // is drawn and nothing kept, so the next visit asks again.
            guard let painted = Self.repaint(data, ramp: ramp) else {
                reply.send(nil, URLError(.cannotDecodeContentData))
                return
            }
            let lifetime = RadarTileCache.lifetime(
                status: http?.statusCode, cacheControl: http?.value(forHTTPHeaderField: "Cache-Control"))
            Self.cache.store(painted, for: key, lifetime: lifetime, now: .now)
            reply.send(painted, nil)
        }.resume()
    }

    /// The PNG `data` with its n0q colors repainted from `ramp`, or `nil` when it
    /// doesn't decode.
    static func repaint(_ data: Data, ramp: [RGBA]) -> Data? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
            let image = CGImageSourceCreateImageAtIndex(source, 0, nil),
            let space = CGColorSpace(name: CGColorSpace.sRGB)
        else { return nil }
        let width = image.width, height = image.height
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        let info = CGImageAlphaInfo.premultipliedLast.rawValue
        let drawn = pixels.withUnsafeMutableBytes { buffer -> Bool in
            guard
                let context = CGContext(
                    data: buffer.baseAddress, width: width, height: height, bitsPerComponent: 8,
                    bytesPerRow: width * 4, space: space, bitmapInfo: info)
            else { return false }
            // Exact pixels: no smoothing between the palette's colors.
            context.interpolationQuality = .none
            context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
            return true
        }
        guard drawn else { return nil }
        RadarPalette.recolor(&pixels, ramp: ramp)
        let out = NSMutableData()
        let made = pixels.withUnsafeMutableBytes { buffer -> Bool in
            guard
                let context = CGContext(
                    data: buffer.baseAddress, width: width, height: height, bitsPerComponent: 8,
                    bytesPerRow: width * 4, space: space, bitmapInfo: info),
                let painted = context.makeImage(),
                let destination = CGImageDestinationCreateWithData(
                    out as CFMutableData, UTType.png.identifier as CFString, 1, nil)
            else { return false }
            CGImageDestinationAddImage(destination, painted, nil)
            return CGImageDestinationFinalize(destination)
        }
        return made ? out as Data : nil
    }
}

/// MapKit's tile callback, carried into URLSession's completion handler: MapKit
/// accepts the call from any thread.
nonisolated private struct TileReply: @unchecked Sendable {
    let send: (Data?, (any Error)?) -> Void
}
