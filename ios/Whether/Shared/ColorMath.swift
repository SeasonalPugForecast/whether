import Foundation

/// A color as sRGB components in 0…1, with alpha: what a theme stores and the
/// color math works on (no SwiftUI here, so the tests and the widget share it).
nonisolated struct RGBA: Hashable, Sendable {
    var red: Double
    var green: Double
    var blue: Double
    var alpha: Double = 1

    init(red: Double, green: Double, blue: Double, alpha: Double = 1) {
        self.red = red
        self.green = green
        self.blue = blue
        self.alpha = alpha
    }

    /// `0xRRGGBB`, opaque.
    init(rgb: UInt32) {
        self.init(
            red: Double(rgb >> 16 & 0xFF) / 255, green: Double(rgb >> 8 & 0xFF) / 255,
            blue: Double(rgb & 0xFF) / 255)
    }

    /// `0xRRGGBBAA`.
    init(rgba: UInt32) {
        self.init(
            red: Double(rgba >> 24 & 0xFF) / 255, green: Double(rgba >> 16 & 0xFF) / 255,
            blue: Double(rgba >> 8 & 0xFF) / 255, alpha: Double(rgba & 0xFF) / 255)
    }

    /// `#RRGGBB` or `#RRGGBBAA` (the `#` is optional); `nil` for anything else.
    init?(hex: String) {
        var digits = Substring(hex.trimmingCharacters(in: .whitespaces))
        if digits.hasPrefix("#") { digits = digits.dropFirst() }
        guard digits.count == 6 || digits.count == 8, digits.allSatisfy(\.isHexDigit),
            let value = UInt32(digits, radix: 16)
        else { return nil }
        if digits.count == 6 { self.init(rgb: value) } else { self.init(rgba: value) }
    }

    /// `#RRGGBB`, or `#RRGGBBAA` when not opaque; upper case, each component
    /// rounded to the nearest of 256 steps and clamped to 0…1 first.
    var hex: String {
        func byte(_ c: Double) -> String {
            String(format: "%02X", Int((min(max(c, 0), 1) * 255).rounded()))
        }
        let rgb = "#" + byte(red) + byte(green) + byte(blue)
        return byte(alpha) == "FF" ? rgb : rgb + byte(alpha)
    }

    /// The same color with each component clamped to 0…1 (out-of-range values
    /// come from math, or from a file someone wrote by hand).
    var clamped: RGBA {
        func c(_ x: Double) -> Double { x.isFinite ? min(max(x, 0), 1) : 0 }
        return RGBA(red: c(red), green: c(green), blue: c(blue), alpha: c(alpha))
    }

    static let white = RGBA(red: 1, green: 1, blue: 1)
    static let black = RGBA(red: 0, green: 0, blue: 0)
}

/// A color in Björn Ottosson's OKLab: `l` is perceived lightness (0 black … 1
/// white), `a` green–red and `b` blue–yellow. Euclidean distance is ΔE_OK.
nonisolated struct OKLab: Equatable, Sendable {
    var l: Double
    var a: Double
    var b: Double

    /// Colorfulness: 0 for grays.
    var chroma: Double { (a * a + b * b).squareRoot() }
}

/// A color vision deficiency to simulate. `achromat` keeps luminance only (no
/// color at all).
nonisolated enum Deficiency: String, CaseIterable, Sendable {
    case protan, deutan, tritan, achromat
}

/// sRGB ↔ linear ↔ OKLab, WCAG contrast and color-vision simulation.
nonisolated enum ColorMath {
    /// An sRGB component (0…1) to linear light.
    static func linear(_ c: Double) -> Double {
        c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4)
    }

    /// Linear light (clamped to 0…1) to an sRGB component.
    static func encoded(_ c: Double) -> Double {
        let c = min(max(c, 0), 1)
        return c <= 0.0031308 ? 12.92 * c : 1.055 * pow(c, 1 / 2.4) - 0.055
    }

    /// WCAG relative luminance (alpha ignored).
    static func luminance(_ c: RGBA) -> Double {
        0.2126 * linear(c.red) + 0.7152 * linear(c.green) + 0.0722 * linear(c.blue)
    }

    /// The WCAG contrast ratio, 1…21 (alpha ignored: composite with `over` first).
    static func contrast(_ x: RGBA, _ y: RGBA) -> Double {
        let lx = luminance(x), ly = luminance(y)
        return (max(lx, ly) + 0.05) / (min(lx, ly) + 0.05)
    }

    /// `top` painted over the opaque `bottom` (sRGB alpha blending, as the screen does).
    static func over(_ top: RGBA, _ bottom: RGBA) -> RGBA {
        let a = top.alpha
        return RGBA(
            red: top.red * a + bottom.red * (1 - a), green: top.green * a + bottom.green * (1 - a),
            blue: top.blue * a + bottom.blue * (1 - a))
    }

    /// `x` moved `t` (0…1) of the way to `y`, per sRGB component.
    static func mix(_ x: RGBA, _ y: RGBA, _ t: Double) -> RGBA {
        RGBA(
            red: x.red + (y.red - x.red) * t, green: x.green + (y.green - x.green) * t,
            blue: x.blue + (y.blue - x.blue) * t, alpha: x.alpha + (y.alpha - x.alpha) * t)
    }

    /// OKLab of a color (alpha ignored).
    static func oklab(_ c: RGBA) -> OKLab {
        oklab(linear: (linear(c.red), linear(c.green), linear(c.blue)))
    }

    private static func oklab(linear c: (Double, Double, Double)) -> OKLab {
        let (r, g, b) = c
        let l = cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b)
        let m = cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b)
        let s = cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b)
        return OKLab(
            l: 0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
            a: 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
            b: 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s)
    }

    /// The sRGB color for an OKLab value, clamped into the sRGB gamut.
    static func rgb(_ lab: OKLab) -> RGBA {
        let l = pow(lab.l + 0.3963377774 * lab.a + 0.2158037573 * lab.b, 3)
        let m = pow(lab.l - 0.1055613458 * lab.a - 0.0638541728 * lab.b, 3)
        let s = pow(lab.l - 0.0894841775 * lab.a - 1.2914855480 * lab.b, 3)
        return RGBA(
            red: encoded(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
            green: encoded(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
            blue: encoded(-0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s))
    }

    /// Machado, Oliveira and Fernandes (2009), severity 1.0, applied to linear RGB.
    private static let machado: [Deficiency: [[Double]]] = [
        .protan: [
            [0.152286, 1.052583, -0.204868], [0.114503, 0.786281, 0.099216],
            [-0.003882, -0.048116, 1.051998],
        ],
        .deutan: [
            [0.367322, 0.860646, -0.227968], [0.280085, 0.672501, 0.047413],
            [-0.011820, 0.042940, 0.968881],
        ],
        .tritan: [
            [1.255528, -0.076749, -0.178779], [-0.078411, 0.930809, 0.147602],
            [0.004733, 0.691367, 0.303900],
        ],
    ]

    /// The simulation of `d` as a 3×3 matrix on linear RGB (rows are red, green, blue):
    /// Machado's, or luminance alone for an achromat. For whole-screen previews.
    static func simulationMatrix(_ d: Deficiency) -> [[Double]] {
        machado[d] ?? Array(repeating: [0.2126, 0.7152, 0.0722], count: 3)
    }

    private static func simulatedLinear(_ c: RGBA, _ d: Deficiency) -> (Double, Double, Double) {
        let v = [linear(c.red), linear(c.green), linear(c.blue)]
        guard let m = machado[d] else {
            // Achromat: luminance alone.
            let y = 0.2126 * v[0] + 0.7152 * v[1] + 0.0722 * v[2]
            return (y, y, y)
        }
        func row(_ i: Int) -> Double { min(max(m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2], 0), 1) }
        return (row(0), row(1), row(2))
    }

    /// How `c` looks to a reader with deficiency `d` (alpha kept).
    static func simulate(_ c: RGBA, _ d: Deficiency) -> RGBA {
        let (r, g, b) = simulatedLinear(c, d)
        return RGBA(red: encoded(r), green: encoded(g), blue: encoded(b), alpha: c.alpha)
    }

    /// ΔE_OK between two colors, as a reader with `deficiency` sees them (or
    /// with typical vision for `nil`). Alpha is ignored.
    static func deltaE(_ x: RGBA, _ y: RGBA, as deficiency: Deficiency? = nil) -> Double {
        func lab(_ c: RGBA) -> OKLab {
            guard let deficiency else { return oklab(c) }
            return oklab(linear: simulatedLinear(c, deficiency))
        }
        let p = lab(x), q = lab(y)
        let (dl, da, db) = (p.l - q.l, p.a - q.a, p.b - q.b)
        return (dl * dl + da * da + db * db).squareRoot()
    }
}
