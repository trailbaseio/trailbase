// swift-tools-version: 6.1
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "TrailBase",
    platforms: [
        .iOS(.v13),
        .macCatalyst(.v13),
        .macOS(.v10_15),
        .watchOS(.v6),
        .tvOS(.v13),
    ],
    products: [
        .library(
            name: "TrailBase",
            targets: ["TrailBase"]),
    ],
    targets: [
        .target(
            name: "TrailBase"),
        .testTarget(
            name: "TrailBaseTests",
            dependencies: ["TrailBase"]
        ),
    ]
)
