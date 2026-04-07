plugins {
    alias(libs.plugins.kotlin.multiplatform)
    alias(libs.plugins.android.kotlin.multiplatform.library)
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.maven.publish)
}

kotlin {
    jvm()
    iosX64()
    iosArm64()
    iosSimulatorArm64()
    macosX64()
    macosArm64()

    js(IR) { browser() }
    wasmJs { browser() }

    android {
        namespace = "io.trailbase.client"
        compileSdk { version = release(36) }
    }

    sourceSets {
        commonMain.dependencies {
            implementation(libs.ktor.client.core)
            implementation(libs.ktor.client.cio)
            implementation(libs.ktor.client.negotiation)
            implementation(libs.ktor.serialization.json)
        }

        jvmTest.dependencies {
            implementation(kotlin("test"))
            implementation(libs.kotlinx.coroutines.test)
            implementation(libs.junit.jupiter)
            implementation(libs.totp)
            runtimeOnly(libs.junit.jupiter.engine)
        }
    }
}

tasks { named<Test>("jvmTest") { useJUnitPlatform() } }

group = "io.trailbase"

version = "0.5.0"

mavenPublishing {
    publishToMavenCentral()

    signAllPublications()

    coordinates(group.toString(), "trailbase-client", version.toString())

    pom {
        name = "TrailBase"
        description = "The official TrailBase kotlin client library"
        url = "https://trailbase.io"
        licenses {
            license {
                name = "Apache-2.0"
                url = "https://opensource.org/license/apache-2-0"
                distribution = "https://opensource.org/license/apache-2-0"
            }
            license {
                name = "OSL-3.0"
                url = "https://opensource.org/license/osl-3-0"
                distribution = "https://opensource.org/license/osl-3-0"
            }
        }
        developers {
            developer {
                id = "sebastian"
                name = "Sebastian"
                email = "contact@trailbase.io"
            }
        }
        scm { url = "https://github.com/trailbaseio/trailbase" }
    }
}
