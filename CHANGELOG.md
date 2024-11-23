## v0.2.3

* Interleaving of multiple HTTP requests into busy v8 isolates/workers.
* JS runtime:
  *  add `addPeriodicCallback` function to register periodic tasks that
     executes on a single worker/isolate.
  *  Constrain method TS argument type (`MethodType`).

## v0.2.2

* Enable "web" APIs in JS runtime.
* Add "Facebook" and "Microsoft" OAuth providers.

## v0.2.1

* Allow setting the number V8 isolates (i.e. JS runtime threads) via
  `--js-runtime-threads`.

## v0.2.0

* Add JS/ES6/TS scripting support based on speedy V8-engine and rustyscript runtime.
  * Enables the registration of custom HTML end-points
  * Provides database access.
  * In the future we'd like to add more life-cycles (e.g. scheduled
    operations).
  * In our [micro-benchmarks](https://trailbase.io/reference/benchmarks/) V8
    was about 45x faster than goja.
* Added official C#/.NET client. Can be used with MAUI for cross-platform
  mobile and desktop development.

## v0.1.1

* Changed license to OSI-approved weak-copyleft OSL-3.0.
* Add GitHub action workflow to automatically build and publish binary releases
  for Linux adm64 as well as MacOS intel and apple arm silicone.
* Add support for geoip database to map client-ips to countries and draw a world map.

## v0.1.0

* Initial release.
