import XCTest

@testable import WhetherKit

/// The NEXRAD n0q palette read back as reflectivity, and tiles repainted from a radar
/// ramp. Expected colors are rows of pyIEM's `composite_n0q.txt`.
final class RadarPaletteTests: XCTestCase {
    private let ramp = (1...5).map { RGBA(rgb: UInt32($0) * 0x111111) }

    func testTheTableIsIEMsN0qRamp() {
        XCTAssertEqual(RadarPalette.n0q.count, 255, "indices 1 to 255")
        XCTAssertEqual(Set(RadarPalette.n0q).count, 255, "every color names one reflectivity")
        XCTAssertEqual(RadarPalette.dbz(index: 1), -32)
        XCTAssertEqual(RadarPalette.dbz(index: 255), 95)
        // (dBZ, r, g, b) rows of composite_n0q.txt.
        for (dbz, rgb) in [
            (-32.0, 0x85718F), (5.0, 0x6376A8), (20.0, 0x43D67E), (35.0, 0x327308), (40.0, 0xFFE200),
            (45.0, 0xFFAC00), (50.0, 0xF80000), (55.0, 0xAA0000), (65.0, 0xF960FA), (75.0, 0x05EAF0),
        ] as [(Double, UInt32)] {
            XCTAssertEqual(RadarPalette.reflectivity[rgb], dbz, String(rgb, radix: 16))
        }
    }

    func testStepsSplitTheReflectivity() {
        XCTAssertEqual(
            [-10, 19.5, 20, 34.5, 35, 44.5, 45, 54.5, 55, 75].map(RadarPalette.step(dbz:)),
            [0, 0, 1, 1, 2, 2, 3, 3, 4, 4])
    }

    func testANearColorReadsAsTheTables() {
        // A decoder's rounding (one or two levels off) still reads as 40 dBZ.
        XCTAssertEqual(RadarPalette.reflectivity(red: 0xFE, green: 0xE3, blue: 0x01), 40)
        XCTAssertNil(RadarPalette.reflectivity(red: 0, green: 0, blue: 0), "black isn't radar")
    }

    func testRepaintingKeepsTransparencyAndForeignColors() {
        var pixels: [UInt8] = [
            0, 0, 0, 0,  // no data
            0xFF, 0xE2, 0x00, 0xFF,  // 40 dBZ: heavy (step 3)
            0x43, 0xD6, 0x7E, 0xFF,  // 20 dBZ: moderate (step 2)
            0xF9, 0x60, 0xFA, 0xFF,  // 65 dBZ: extreme (step 5)
            0x63, 0x76, 0xA8, 0xFF,  // 5 dBZ: light (step 1)
            0x85, 0x71, 0x8F, 0xFF,  // -32 dBZ: a weak echo, at half strength
            0x00, 0x00, 0x00, 0xFF,  // black: not n0q's
        ]
        XCTAssertEqual(RadarPalette.recolor(&pixels, ramp: ramp), 5)
        XCTAssertEqual(Array(pixels[0..<4]), [0, 0, 0, 0])
        XCTAssertEqual(Array(pixels[4..<8]), [0x33, 0x33, 0x33, 0xFF])
        XCTAssertEqual(Array(pixels[8..<12]), [0x22, 0x22, 0x22, 0xFF])
        XCTAssertEqual(Array(pixels[12..<16]), [0x55, 0x55, 0x55, 0xFF])
        XCTAssertEqual(Array(pixels[16..<20]), [0x11, 0x11, 0x11, 0xFF])
        // Premultiplied at half strength: 0x11 / 2, rounded either way.
        XCTAssertEqual(pixels[23], 0x80)
        XCTAssertTrue((8...9).contains(pixels[20]) && pixels[20] == pixels[21] && pixels[21] == pixels[22])
        XCTAssertEqual(Array(pixels[24..<28]), [0, 0, 0, 0xFF])
        var short: [UInt8] = [0xFF, 0xE2, 0x00, 0xFF]
        XCTAssertEqual(RadarPalette.recolor(&short, ramp: Array(ramp.prefix(3))), 0, "a ramp needs five steps")
    }

    func testOnlyNEXRADTilesAreRepainted() {
        XCTAssertTrue(
            RadarPalette.recolors(
                template: "https://mesonet.agron.iastate.edu/cache/tile.py/1.0.0/nexrad-n0q-900913-m10m/{z}/{x}/{y}.png"))
        XCTAssertFalse(
            RadarPalette.recolors(template: "https://tile.openweathermap.org/map/precipitation_new/{z}/{x}/{y}.png"))
        XCTAssertNotEqual(RadarPalette.key(ramp), RadarPalette.key(ramp.reversed()), "cached per ramp")
    }

    // MARK: Repainted tiles in memory

    func testATileGoesStaleWithTheRadar() {
        // IEM's URLs are relative to now: the tile kept at 14:00 isn't 15:30's radar.
        let cache = RadarTileCache()
        let t0 = Date(timeIntervalSince1970: 1_800_000_000)
        let tile = Data([1, 2, 3])
        cache.store(tile, for: "ramp|url", lifetime: RadarTileCache.maxLifetime, now: t0)
        XCTAssertEqual(cache.tile(for: "ramp|url", now: t0.addingTimeInterval(60)), tile, "stepping the loop again")
        XCTAssertNil(cache.tile(for: "ramp|url", now: t0.addingTimeInterval(90 * 60)), "a later visit")
        XCTAssertNil(cache.tile(for: "ramp|url", now: t0), "a stale tile is dropped")
        cache.store(tile, for: "ramp|other", lifetime: 0, now: t0)
        XCTAssertNil(cache.tile(for: "ramp|other", now: t0), "no lifetime keeps nothing")
    }

    func testOnlyFreshSuccessfulTilesAreKept() {
        let most = RadarTileCache.maxLifetime
        XCTAssertEqual(RadarTileCache.lifetime(status: 200, cacheControl: nil), most)
        XCTAssertEqual(RadarTileCache.lifetime(status: 200, cacheControl: "public, max-age=120"), 120)
        XCTAssertEqual(RadarTileCache.lifetime(status: 200, cacheControl: "max-age=86400"), most, "one update at most")
        XCTAssertEqual(RadarTileCache.lifetime(status: 200, cacheControl: "no-cache"), 0)
        XCTAssertEqual(RadarTileCache.lifetime(status: 200, cacheControl: "private, No-Store"), 0)
        XCTAssertEqual(RadarTileCache.lifetime(status: 503, cacheControl: "max-age=300"), 0, "an error page")
        XCTAssertEqual(RadarTileCache.lifetime(status: 404, cacheControl: nil), 0)
        XCTAssertTrue(RadarTileCache.succeeded(status: 204))
        XCTAssertTrue(RadarTileCache.succeeded(status: nil))
        XCTAssertFalse(RadarTileCache.succeeded(status: 503))
    }

    // MARK: The legend describes the map

    func testNOAAsTilesStayOnlyUnderTheirOwnLegend() {
        let whether = ColorPresets.whether
        XCTAssertNil(RadarPalette.repaintRamp(whether, vision: .typical, patterns: false), "NOAA's colors, gradient")
        XCTAssertEqual(RadarPalette.noaa.map(\.hex), ["#00C700", "#FFD900", "#FF8C00", "#E60000", "#BF00BF"])
        // The stepped legend's words follow `stepStarts`, not NOAA's color bands: repaint.
        XCTAssertEqual(RadarPalette.repaintRamp(whether, vision: .typical, patterns: true), RadarPalette.noaa)
        for vision in ColorVision.allCases where vision != .typical {
            let theme = ColorPresets.resolve(id: "whether", vision: vision, month: 10)
            XCTAssertEqual(
                RadarPalette.repaintRamp(theme, vision: vision, patterns: false),
                ColorRole.radar.map { theme[$0].light }, vision.rawValue)
        }
        // Every other preset paints the legend in its own colors, so the map follows.
        for theme in ColorPresets.all where theme.id != "whether" {
            XCTAssertEqual(
                RadarPalette.repaintRamp(theme, vision: .typical, patterns: false),
                ColorRole.radar.map { theme[$0].light }, theme.id)
        }
        // A custom theme from Whether keeps NOAA's tiles until its radar colors change.
        var mine = ColorTheme(id: "custom-1", name: "Mine", base: "whether", colors: [:]).completed(from: whether)
        XCTAssertNil(RadarPalette.repaintRamp(mine, vision: .typical, patterns: false))
        mine.colors[.radar3] = ThemeColor(light: RGBA(rgb: 0x123456))
        XCTAssertNotNil(RadarPalette.repaintRamp(mine, vision: .typical, patterns: false))
    }
}
