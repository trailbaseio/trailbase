import org.gradle.api.tasks.testing.logging.TestExceptionFormat
import org.gradle.api.tasks.testing.logging.TestLogEvent

plugins {
    alias(libs.plugins.android.kotlin.multiplatform.library) apply false
    alias(libs.plugins.kotlin.jvm) apply false
    alias(libs.plugins.kotlin.multiplatform) apply false
    alias(libs.plugins.kotlin.serialization) apply false
    alias(libs.plugins.maven.publish) apply false

    // Code formatting, linting, ...
    alias(libs.plugins.spotless)
}

allprojects {
    tasks.withType<Test> {
        testLogging {
            showStandardStreams = true
            showExceptions = true
            showCauses = true
            events =
                setOf(
                    TestLogEvent.PASSED,
                    TestLogEvent.SKIPPED,
                    TestLogEvent.FAILED,
                )
            exceptionFormat = TestExceptionFormat.FULL
        }
        outputs.upToDateWhen { false }
    }
}

spotless {
    kotlin {
        target("**/*.kt", "**/*.kts")
        ktlint()

        suppressLintsFor {
            step = "ktlint"
            shortCode = "standard:no-wildcard-imports"
        }
    }
}
