// NOTE: The "official" plugin: "libs.kotlin.gradle.plugin" for building
// "convention plugins" cannot access the versions catalog, this is why we're
// using the below settings plugin. See also:
//
// https://github.com/gradle/gradle/issues/15383, .
// https://docs.gradle.org/current/userguide/version_catalogs.html#sec:buildsrc-version-catalog.
plugins {
    id("dev.panuszewski.typesafe-conventions") version "0.8.0"
}
