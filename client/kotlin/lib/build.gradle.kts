import org.gradle.api.tasks.testing.logging.TestExceptionFormat
import org.gradle.api.tasks.testing.logging.TestLogEvent

plugins {
    alias(libs.plugins.kotlin.jvm)
    alias(libs.plugins.kotlin.serialization)

    // Code formatting, linting, ...
    alias(libs.plugins.spotless)

    // Apply the java-library plugin for API and implementation separation.
    `java-library`
}

repositories {
    // Use Maven Central for resolving dependencies.
    mavenCentral()
}

dependencies {
    implementation(libs.ktor.client.core)
    implementation(libs.ktor.client.cio)
    implementation(libs.ktor.client.negotiation)
    implementation(libs.ktor.serialization.json)

    implementation(libs.kotlinx.serialization.json)

    testImplementation(libs.kotlinx.coroutines.test)

    // This dependency is exported to consumers, that is to say found on their compile
    // classpath.
    api(libs.commons.math3)

    // This dependency is used internally, and not exposed to consumers on their own compile
    // classpath.
    implementation(libs.guava)
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

allprojects {
    tasks.withType<Test> {
        testLogging {
            showStandardStreams = true
            showExceptions = true
            showCauses = true
            events = setOf(TestLogEvent.PASSED, TestLogEvent.SKIPPED, TestLogEvent.FAILED)
            exceptionFormat = TestExceptionFormat.FULL
        }
        outputs.upToDateWhen { false }
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

// Apply a specific Java toolchain to ease working on different environments.
java { toolchain { languageVersion = JavaLanguageVersion.of(21) } }
