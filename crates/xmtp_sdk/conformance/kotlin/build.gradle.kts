plugins {
    kotlin("jvm") version "2.0.0"
    application
}

dependencies {
    implementation("net.java.dev.jna:jna:5.17.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.0")
}

kotlin { jvmToolchain(21) }
sourceSets.main { kotlin.srcDir("../../../../target/sdk-conformance/kotlin/uniffi") }
sourceSets.main { kotlin.srcDir("../../../../target/sdk-conformance/kotlin/runtime") }

application { mainClass.set("ConformanceKt") }
tasks.named<JavaExec>("run") {
    jvmArgs("-Xss16m", "-Djna.library.path=${project.rootDir}/../../../../target/debug")
    environment("RUST_MIN_STACK", "16777216")
    environment("SDK_SIGN_KEY", System.getenv("SDK_SIGN_KEY"))
    environment("SDK_NODE_BIN", System.getenv("SDK_NODE_BIN"))
    environment("SDK_SIGN_SCRIPT", System.getenv("SDK_SIGN_SCRIPT"))
}
