plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "org.xmtp.sdk.staged"
    compileSdk = 35
    defaultConfig { minSdk = 23 }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    sourceSets["main"].java.srcDirs(
        "../../../../../../target/sdk-public/kotlin/uniffi",
        "../../../../../../target/sdk-public/kotlin/runtime",
        "../../../../../../target/sdk-public/kotlin/android",
    )
}

kotlin { compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17) } }

dependencies {
    implementation("net.java.dev.jna:jna:5.17.0@aar")
    api("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.0")
}
