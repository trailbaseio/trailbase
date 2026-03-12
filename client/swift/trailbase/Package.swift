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
      targets: ["TrailBase"])
  ],
  dependencies: [
    .package(url: "https://github.com/swiftlang/swift-subprocess.git", branch: "main"),
    .package(url: "https://github.com/lachlanbell/SwiftOTP.git", .upToNextMinor(from: "3.0.0")),
  ],
  targets: [
    .target(
      name: "TrailBase"),
    .testTarget(
      name: "TrailBaseTests",
      dependencies: [
        "TrailBase",
        .product(name: "SwiftOTP", package: "SwiftOTP"),
        .product(name: "Subprocess", package: "swift-subprocess"),
      ]
    ),
  ]
)
