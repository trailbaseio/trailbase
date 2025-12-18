test:
	gradle test

# Note `gradle build` also runs the tests.
build:
	gradle assemble

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
	gradle publishToMavenCentral

release:
	source .env && gradle publishAndReleaseToMavenCentral

format:
	gradle spotlessApply

list:
	gradle tasks

clean:
	rm -rf lib/build/


.PHONY: test build publish release format list clean
