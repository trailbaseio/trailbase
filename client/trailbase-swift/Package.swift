// swift-tools-version: 6.1
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "trailbase",
    platforms: [
        .iOS(.v13),
        .macCatalyst(.v13),
        .macOS(.v10_15),
        .watchOS(.v6),
        .tvOS(.v13),
    ],
    products: [
        .library(
            name: "trailbase",
            targets: ["trailbase"]),
    ],
    targets: [
        .target(
            name: "trailbase"),
        .testTarget(
            name: "trailbaseTests",
            dependencies: ["trailbase"]
        ),
    ]
)
