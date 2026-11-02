// swift-tools-version:5.9
// WhetherKit: the Rust core (whether-ffi) for the iOS app.
// Build the XCFramework and bindings first: `cargo xtask ios build` (on the Mac).

import PackageDescription

let package = Package(
    name: "WhetherKit",
    platforms: [.iOS(.v17), .macOS(.v14)],
    products: [
        .library(name: "WhetherKit", targets: ["WhetherKit"]),
    ],
    targets: [
        .binaryTarget(name: "whether_ffiFFI", path: "WhetherFFI.xcframework"),
        .target(name: "WhetherKit", dependencies: ["whether_ffiFFI"]),
        .testTarget(
            name: "WhetherKitTests", dependencies: ["WhetherKit"],
            resources: [.copy("Resources")]),
    ]
)
