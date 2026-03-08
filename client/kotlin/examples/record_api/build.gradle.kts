plugins {
    alias(libs.plugins.kotlin.jvm)
    alias(libs.plugins.kotlin.serialization)

    application
}

application { mainClass = "MainKt" }

dependencies {
    implementation(project(":lib"))

    implementation(libs.ktor.serialization.json)
    implementation(libs.kotlinx.coroutines.core)
}
