import Foundation

/// The NEXRAD composite's colors, read back as reflectivity, and the radar tiles
/// repainted from a theme's radar ramp for color vision.
///
/// The Iowa Environmental Mesonet draws the n0q composite (`nexrad-n0q-900913`
/// tiles) with a fixed 255-color table: palette index `i` (1…255) is
/// `-32 + 0.5 × (i − 1)` dBZ, -32 to 95 dBZ, and index 0 is no data (transparent).
/// The table below is IEM's own, copied from pyIEM
/// `src/pyiem/data/ramps/composite_n0q.txt` (github.com/akrherz/pyIEM, last changed in
/// commit 1be309566535, 2021-02-09; `util/make_ramps.py` exports it from IEM's
/// `iemrasters_lookup` table for `composite_n0q`), and the ramp is documented at
/// mesonet.agron.iastate.edu/docs/nexrad_mosaic/ ("-32 dBZ through 90 dBZ 0.5 dBZ
/// increment"). Checked against live tiles on 2026-10-25 (zooms 4 to 8): the tiles are
/// RGBA PNGs whose opaque pixels are all exactly table colors (nearest-neighbour
/// resampling) and whose other pixels are fully transparent.
nonisolated enum RadarPalette {
    /// The n0q colors as `0xRRGGBB`, palette index 1 (-32 dBZ) first, 0.5 dBZ apart.
    static let n0q: [UInt32] = [
        0x85718F, 0x85728F, 0x86738D, 0x87758B, 0x87768B, 0x887789, 0x897987, 0x897A87,
        0x8A7B85, 0x8B7D84, 0x8B7E84, 0x8C7F82, 0x8D8180, 0x8D8280, 0x8E837E, 0x8F847C,
        0x8F857C, 0x90877B, 0x918879, 0x918979, 0x928B77, 0x938D75, 0x969153, 0x989457,
        0x9B975B, 0x9D9A60, 0xA09D64, 0xA3A068, 0xA5A36D, 0xA8A671, 0xAAA976, 0xADAC7A,
        0xB0AF7E, 0xB2B283, 0xB7B88C, 0xBABB90, 0xBDBE94, 0xBFC199, 0xC2C49D, 0xC4C7A2,
        0xC7CAA6, 0xCACDAA, 0xCCD0AF, 0xD2D4B4, 0xCFD2B4, 0xC9CCB4, 0xC6C9B4, 0xC3C7B4,
        0xC0C4B4, 0xBDC1B4, 0xB9BEB4, 0xB6BBB4, 0xB3B9B4, 0xB0B6B4, 0xADB3B4, 0xAAB0B4,
        0xA4ABB4, 0xA0A8B4, 0x9DA5B4, 0x9AA2B4, 0x97A0B4, 0x949DB4, 0x919AB4, 0x949BB5,
        0x9098B4, 0x8C95B3, 0x8892B2, 0x808CB0, 0x7C89AF, 0x7886AE, 0x7483AC, 0x7080AB,
        0x6C7DAA, 0x6779A9, 0x6376A8, 0x5F73A7, 0x5B70A6, 0x576DA4, 0x4F67A2, 0x4B64A1,
        0x4761A0, 0x435E9F, 0x415B9E, 0x4361A2, 0x4568A6, 0x486FAA, 0x4A76AE, 0x4D7DB2,
        0x4F84B6, 0x518BBB, 0x5699C3, 0x599FC7, 0x5BA6CB, 0x5EADCF, 0x60B4D4, 0x62BBD8,
        0x65C2DC, 0x67C9E0, 0x6AD0E4, 0x6FD6E8, 0x68D6D7, 0x59D6B3, 0x52D6A2, 0x4BD690,
        0x43D67E, 0x3CD66D, 0x35D65B, 0x11D518, 0x11D117, 0x10CD17, 0x10C816, 0x10C416,
        0x0FBC15, 0x0FB714, 0x0EB314, 0x0EAF13, 0x0EAB13, 0x0DA612, 0x0DA212, 0x0D9E11,
        0x0C9911, 0x0C9510, 0x0C9110, 0x0B880F, 0x0B840E, 0x0A800E, 0x0A7C0D, 0x0A770D,
        0x09730C, 0x096F0C, 0x096B0B, 0x08660B, 0x08620A, 0x095E09, 0x327308, 0x467D08,
        0x5B8807, 0x6F9207, 0x849D06, 0x98A806, 0xADB205, 0xC1BD05, 0xD6C704, 0xEAD204,
        0xFFE200, 0xFFD800, 0xFFD300, 0xFFCE00, 0xFFC900, 0xFFC400, 0xFFC000, 0xFFBB00,
        0xFFB600, 0xFFB100, 0xFFAC00, 0xFFA700, 0xFFA200, 0xFF9900, 0xFF9400, 0xFF8F00,
        0xFF8A00, 0xFF8500, 0xFF8000, 0xFF0000, 0xF80000, 0xF10000, 0xEA0000, 0xE30000,
        0xD50000, 0xCD0000, 0xC60000, 0xBF0000, 0xB80000, 0xB10000, 0xAA0000, 0xA30000,
        0x9B0000, 0x940000, 0x8D0000, 0x7F0000, 0x780000, 0x710000, 0xFFFFFF, 0xFFF5FF,
        0xFFEAFF, 0xFFDFFF, 0xFFD4FF, 0xFFC9FF, 0xFFBEFF, 0xFFB3FF, 0xFF9DFF, 0xFF92FF,
        0xFF75FF, 0xFC6BFD, 0xF960FA, 0xF656F7, 0xF34BF4, 0xF040F1, 0xED36EF, 0xEA2BEC,
        0xE720E9, 0xE10BE3, 0xB200FF, 0xAC00FC, 0xA400F7, 0x9B00F4, 0x9300EF, 0x8800EA,
        0x8300E8, 0x7900E2, 0x7200DD, 0x6900DB, 0x05ECF0, 0x05EBF0, 0x05EAF0, 0x05DDE0,
        0x05DCE0, 0x05DBE0, 0x05CDD0, 0x05CCD0, 0x04BDC0, 0x04BCC0, 0x04BBC0, 0x04AEB0,
        0x04ADB0, 0x049EA0, 0x049DA0, 0x049CA0, 0x038E90, 0x038D90, 0x038C90, 0x037E80,
        0x037D80, 0x036F70, 0x036E70, 0x036D70, 0x025F60, 0x025E60, 0x024F50, 0x024E50,
        0x024D50, 0x023F40, 0x023E40, 0x023D40, 0x013030, 0x012F30, 0x012020, 0x011F20,
        0x011E20, 0x3A67B5, 0x3A66B5, 0x3A65B5, 0x3A64B5, 0x3A63B5, 0x3A62B5,
    ]

    /// The reflectivity of palette index `index` (1…255), in dBZ.
    static func dbz(index: Int) -> Double { -32 + 0.5 * Double(index - 1) }

    /// Where steps 2 to 5 of the legend begin, in dBZ: light below 20, moderate to 35,
    /// heavy to 45, very heavy to 55, extreme (hail likely) from 55.
    static let stepStarts: [Double] = [20, 35, 45, 55]

    /// Below this the composite holds weak echoes (clutter, clear air, the edge of
    /// drizzle) in NOAA's muted grays; repainted, they stay faint.
    static let weak: Double = 5

    /// The legend step (0…4) of a reflectivity.
    static func step(dbz: Double) -> Int {
        stepStarts.lastIndex { dbz >= $0 }.map { $0 + 1 } ?? 0
    }

    /// Each table color's reflectivity.
    static let reflectivity: [UInt32: Double] = Dictionary(
        uniqueKeysWithValues: n0q.enumerated().map { ($0.element, dbz(index: $0.offset + 1)) })

    /// The reflectivity of a pixel's color: the table's, or the nearest table color's
    /// within a small distance (a decoder's rounding), else `nil` (not radar).
    static func reflectivity(red: UInt8, green: UInt8, blue: UInt8) -> Double? {
        let rgb = UInt32(red) << 16 | UInt32(green) << 8 | UInt32(blue)
        if let exact = reflectivity[rgb] { return exact }
        var best: (distance: Int, index: Int)?
        for (i, c) in n0q.enumerated() {
            let d =
                abs(Int(red) - Int(c >> 16 & 0xFF)) + abs(Int(green) - Int(c >> 8 & 0xFF))
                + abs(Int(blue) - Int(c & 0xFF))
            if d <= 6, d < (best?.distance ?? .max) { best = (d, i) }
        }
        return best.map { dbz(index: $0.index + 1) }
    }

    /// Whether tiles from `template` are the n0q composite (OpenWeather's layer has
    /// its own colors and is left as it is).
    static func recolors(template: String) -> Bool { template.contains("/nexrad-n0q") }

    /// Repaints RGBA pixels (8 bits each, alpha premultiplied or not: the tiles' are
    /// 0 or 255) in place: every n0q color becomes its step's color in `ramp` (five,
    /// light to heavy), weak echoes at half strength; transparent pixels and colors
    /// that aren't n0q's stay. Returns how many pixels changed.
    @discardableResult
    static func recolor(_ pixels: inout [UInt8], ramp: [RGBA]) -> Int {
        guard ramp.count == 5 else { return 0 }
        func byte(_ x: Double) -> UInt8 { UInt8((min(max(x, 0), 1) * 255).rounded()) }
        var memo: [UInt32: (UInt8, UInt8, UInt8, UInt8)?] = [:]
        var changed = 0
        var i = 0
        while i + 3 < pixels.count {
            let alpha = pixels[i + 3]
            if alpha > 0 {
                let key = UInt32(pixels[i]) << 16 | UInt32(pixels[i + 1]) << 8 | UInt32(pixels[i + 2])
                let paint: (UInt8, UInt8, UInt8, UInt8)?
                if let known = memo[key] {
                    paint = known
                } else {
                    paint = reflectivity(red: pixels[i], green: pixels[i + 1], blue: pixels[i + 2]).map {
                        let c = ramp[step(dbz: $0)]
                        let a = $0 < weak ? 0.5 : 1
                        // Premultiplied, as the tile loader's bitmap is.
                        return (byte(c.red * a), byte(c.green * a), byte(c.blue * a), byte(a))
                    }
                    memo[key] = paint
                }
                if let (r, g, b, a) = paint {
                    let scale = Double(alpha) / 255
                    pixels[i] = byte(Double(r) / 255 * scale)
                    pixels[i + 1] = byte(Double(g) / 255 * scale)
                    pixels[i + 2] = byte(Double(b) / 255 * scale)
                    pixels[i + 3] = byte(Double(a) / 255 * scale)
                    changed += 1
                }
            }
            i += 4
        }
        return changed
    }

    /// A key for a ramp, for caching repainted tiles per ramp.
    static func key(_ ramp: [RGBA]) -> String { ramp.map(\.hex).joined(separator: ",") }

    /// NOAA's own tile colors, as the Whether preset's radar roles name them (green,
    /// yellow, orange, red, magenta): the gradient legend over unrepainted tiles.
    static var noaa: [RGBA] { ColorRole.radar.map { ColorPresets.whether[$0].light } }

    /// The ramp NEXRAD's tiles are repainted with, or `nil` to keep NOAA's colors
    ///. The legend must describe the map: NOAA's tiles stay only under the
    /// gradient legend in NOAA's colors, that is with typical vision, no patterns and
    /// a theme whose radar colors are Whether's. Every other theme, color vision or
    /// patterns repaints the tiles in the theme's five steps, at the step words' bands
    /// (`stepStarts`), and the legend shows those swatches with their words.
    static func repaintRamp(_ theme: ColorTheme, vision: ColorVision, patterns: Bool) -> [RGBA]? {
        let ramp = ColorRole.radar.map { theme[$0].light }
        return vision == .typical && !patterns && ramp == noaa ? nil : ramp
    }
}
