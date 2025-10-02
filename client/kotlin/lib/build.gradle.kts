import org.gradle.api.tasks.testing.logging.TestExceptionFormat
import org.gradle.api.tasks.testing.logging.TestLogEvent

plugins {
    alias(libs.plugins.kotlin.jvm)
    alias(libs.plugins.kotlin.serialization)

    // Code formatting, linting, ...
    alias(libs.plugins.spotless)

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

group = "io.trailbase"

version = "0.1.0"

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

java {
    toolchain { languageVersion = JavaLanguageVersion.of(22) }

    // Packaging
    // withJavadocJar()
    withSourcesJar()

    modularity.inferModulePath = true
}
