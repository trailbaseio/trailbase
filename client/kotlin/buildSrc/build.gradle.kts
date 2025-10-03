plugins {
    // Support convention plugins written in Kotlin. Convention plugins are build scripts in 'src/main' that automatically become available as plugins in the main build.
    `kotlin-dsl`
}

repositories {
    // Use the plugin portal to apply community plugins in convention plugins.
    gradlePluginPortal()
}

// NOTE: This doesn't support version catalog access, see settings.gradle.kts.
// dependencies {
//     implementation(libs.kotlin.gradle.plugin)
// }
