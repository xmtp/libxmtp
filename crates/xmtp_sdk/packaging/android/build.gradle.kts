import org.gradle.api.artifacts.dsl.LockMode

buildscript {
    configurations.classpath {
        resolutionStrategy.activateDependencyLocking()
    }
    dependencyLocking {
        lockMode.set(LockMode.STRICT)
    }
}

plugins {
    id("com.android.library") version "8.9.1"
    id("org.jetbrains.kotlin.android") version "2.0.0"
}

dependencyLocking {
    lockAllConfigurations()
    lockMode.set(LockMode.STRICT)
}

val generated = providers.environmentVariable("XMTP_SDK_GENERATED_DIR").get()
val native = providers.environmentVariable("XMTP_SDK_ANDROID_JNI_DIR").get()
android {
    namespace = "org.xmtp.sdk"
    compileSdk = 35
    defaultConfig { minSdk = 23 }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    sourceSets["main"].java.srcDirs(
        "$generated/kotlin/uniffi",
        "$generated/kotlin/runtime",
        "$generated/kotlin/android",
    )
    sourceSets["main"].jniLibs.srcDirs(native)
}
kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
        freeCompilerArgs.add("-Xjvm-default=all")
    }
}
dependencies {
    implementation("net.java.dev.jna:jna:5.17.0@aar")
    api("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.0")
}
