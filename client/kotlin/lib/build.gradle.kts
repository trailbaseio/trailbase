import org.gradle.api.tasks.testing.logging.TestExceptionFormat
import org.gradle.api.tasks.testing.logging.TestLogEvent

plugins {
    id("buildlogic.kotlin-common-conventions")

    alias(libs.plugins.maven.publish)

    // Apply the java-library plugin for API and implementation separation.
    id("java-library")
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

    testImplementation(libs.kotlinx.coroutines.test)
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

group = "io.trailbase"

version = "0.2.0"

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
