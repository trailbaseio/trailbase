# Changelog

## 0.5.0

- Support realtime subscriptions with record-based filters. Requires TB ^0.18.1.
- Switch from `dio` to `http`.

## 0.4.0

- More powerful, nested list filters intrdocued with TrailBase v0.12.0.

## 0.3.0

- Add `count` parameter to RecordApi.list to retrieve `total_count` at extra cost.
  Requires TB >= v0.6.0.
- Add `expand` parameter for RecordApi.(list|get) to expand foreign records.
  Requires TB >= v0.6.0.

## 0.2.1

- Fix heartbeat decoding issue with record subscriptions.

## 0.2.0

- Return ListResponse from RecordApi::list.

## 0.1.2

- Add support for "realtime" subscriptions to listen for record changes:
  insertions, updates, deletions.

## 0.1.0

### Features

- Initial client release including support for authentication and record APIs.
