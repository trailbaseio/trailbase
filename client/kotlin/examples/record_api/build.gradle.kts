plugins {
    id("buildlogic.kotlin-common-conventions")

    application
}

application { mainClass = "MainKt" }

dependencies {
    api(project(":lib"))

    implementation(libs.kotlinx.coroutines.core)
}
