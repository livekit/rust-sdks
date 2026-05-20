import org.gradle.api.publish.maven.MavenPublication
import org.gradle.plugins.signing.SigningExtension

plugins {
    id("com.android.library") version "8.2.2"
    id("org.jetbrains.kotlin.android") version "1.9.22"
    `maven-publish`
    signing
    id("io.github.gradle-nexus.publish-plugin") version "2.0.0"
}

group = providers.gradleProperty("GROUP").get()
version = providers.gradleProperty("VERSION_NAME").get()

val generatedKotlinDir = rootDir.resolve("../../packages/kotlin")

android {
    namespace = "io.livekit.uniffi"
    compileSdk = 34

    defaultConfig {
        minSdk = 24

        consumerProguardFiles("consumer-rules.pro")
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    publishing {
        singleVariant("release") {
            withSourcesJar()
        }
    }

    sourceSets {
        getByName("main") {
            if (generatedKotlinDir.exists()) {
                kotlin.srcDir(generatedKotlinDir)
            }
        }
    }
}

dependencies {
    implementation("net.java.dev.jna:jna:5.16.0@aar")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.1")
}

afterEvaluate {
    extensions.configure<PublishingExtension>("publishing") {
        publications {
            register<MavenPublication>("release") {
                from(components["release"])
                artifactId = providers.gradleProperty("POM_ARTIFACT_ID").get()

                pom {
                    name.set(providers.gradleProperty("POM_NAME").get())
                    description.set(providers.gradleProperty("POM_DESCRIPTION").get())
                    packaging = providers.gradleProperty("POM_PACKAGING").get()
                    url.set(providers.gradleProperty("POM_URL").get())
                    licenses {
                        license {
                            name.set(providers.gradleProperty("POM_LICENCE_NAME").get())
                            url.set(providers.gradleProperty("POM_LICENCE_URL").get())
                            distribution.set(providers.gradleProperty("POM_LICENCE_DIST").get())
                        }
                    }
                    developers {
                        developer {
                            id.set(providers.gradleProperty("POM_DEVELOPER_ID").get())
                            name.set(providers.gradleProperty("POM_DEVELOPER_NAME").get())
                        }
                    }
                    scm {
                        connection.set(providers.gradleProperty("POM_SCM_CONNECTION").get())
                        developerConnection.set(providers.gradleProperty("POM_SCM_DEV_CONNECTION").get())
                        url.set(providers.gradleProperty("POM_SCM_URL").get())
                    }
                }
            }
        }
    }

    val releaseSigningEnabled =
        providers
            .gradleProperty("RELEASE_SIGNING_ENABLED")
            .map(String::toBooleanStrict)
            .getOrElse(true)
    if (releaseSigningEnabled) {
        extensions.configure<SigningExtension>("signing") {
            sign(publishing.publications["release"])
        }
    }
}

nexusPublishing {
    repositories {
        register("nexus") {
            nexusUrl.set(uri(providers.gradleProperty("RELEASE_REPOSITORY_URL").get()))
            snapshotRepositoryUrl.set(uri(providers.gradleProperty("SNAPSHOT_REPOSITORY_URL").get()))
            providers.gradleProperty("STAGING_PROFILE_ID").orNull?.takeIf { it.isNotBlank() }?.let { profileId ->
                stagingProfileId.set(profileId)
            }
        }
    }
}
