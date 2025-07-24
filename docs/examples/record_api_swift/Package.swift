// swift-tools-version: 6.1
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
  name: "RecordApiDocs",
  products: [
    .library(
      name: "RecordApiDocs",
      targets: ["RecordApiDocs"])
  ],
  dependencies: [
    .package(path: "../../../client/swift/trailbase")
  ],
  targets: [
    .target(
      name: "RecordApiDocs",
      dependencies: [
        .product(name: "TrailBase", package: "trailbase")
      ]
    ),
    .testTarget(
      name: "RecordApiDocsTests",
      dependencies: ["RecordApiDocs"]
    ),
  ],
)
