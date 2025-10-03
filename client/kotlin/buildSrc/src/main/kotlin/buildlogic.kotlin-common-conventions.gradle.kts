repositories {
    // Use Maven Central for resolving dependencies.
    mavenCentral()
}

plugins {
    // Apply the org.jetbrains.kotlin.jvm Plugin to add support for Kotlin.
    alias(libs.plugins.kotlin.jvm)

    alias(libs.plugins.kotlin.serialization)

    // Code formatting, linting, ...
    alias(libs.plugins.spotless)
}

dependencies {
        implementation(libs.kotlinx.serialization.json)
}

testing {
    suites {
        // Configure the built-in test suite
        val test by
            getting(JvmTestSuite::class) {
                // Use Kotlin Test test framework
                useKotlinTest("2.1.20")
            }
    }
}

spotless {
    kotlin {
        ktfmt().kotlinlangStyle()
        // ktlint()
        // diktat()
    }
    kotlinGradle {
        target("*.gradle.kts")
        ktfmt().kotlinlangStyle()
        // ktlint()
    }
}
