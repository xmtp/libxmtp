plugins {
    id("com.android.library") version "8.9.1"
    kotlin("android") version "2.0.0"
    `maven-publish`
}
group = "org.xmtp"
version = "0.1.0"
android {
    namespace = "org.xmtp.migration"
    compileSdk = 35
    defaultConfig {
        minSdk = 23
        consumerProguardFiles("consumer-rules.pro")
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
    publishing { singleVariant("release") { withSourcesJar() } }
}
dependencies {
    api("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.0")
    implementation("net.java.dev.jna:jna:5.18.1@aar")
}
afterEvaluate {
    publishing {
        publications {
            create<MavenPublication>("release") {
                from(components["release"])
                artifactId = "migration"
            }
        }
    }
}
