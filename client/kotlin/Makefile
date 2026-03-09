test:
	./gradlew allTests

# Note `gradle build` also runs the tests.
build:
	./gradlew assemble

# Version number is in `lib/build.gradle.kts`.
# Tokens are in `~/.gradle/gradle.properties`
#
# Maven Central: https://central.sonatype.com/publishing/deployments
#
# Setting up publishing requires extracting your private key into a binary
# key-ring file with:
#
#   $ gpg --no-armor --export-secret-keys <key_id> > key.gpg
#
# Then generate a token on maven central and set the credentials:
#
#   $ cat ~/.gradle/gradle.properties
#   mavenCentralUsername=<username_from_generated_maven_central_token>
#   mavenCentralPassword=<password_from_generated_maven_central_token>
#   signing.keyId=<key_id>
#   signing.password=<key_password>
#   signing.secretKeyRingFile=</absolute/path/to/key.gpg>
#
# Version specifier lives in `lib/build.gradle.kts`.
#
# NOTE: Only publishes, requires subsequent manual release on maven central.
publish:
	./gradlew publishToMavenCentral

release:
	source .env && gradle publishKotlinMultiplatformPublicationToMavenCentralRepository

format:
	./gradlew spotlessApply

list:
	./gradlew tasks

# Run the example application.
run:
	./gradlew run

clean:
	rm -rf lib/build/


.PHONY: test build publish release format list clean run
