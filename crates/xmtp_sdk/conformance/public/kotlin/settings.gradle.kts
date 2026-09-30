pluginManagement {
    repositories {
        gradlePluginPortal()
        google()
        mavenCentral()
    }
}
dependencyResolutionManagement {
    repositories {
        google()
        mavenCentral()
    }
}
rootProject.name = "xmtp-sdk-public-consumer"
// `sdk` compiles the staged SDK sources as their own Android library, so
// Kotlin `internal` members stay private to it. `consumer` is the app code.
include(":sdk", ":consumer")
