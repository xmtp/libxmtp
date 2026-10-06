plugins {
    kotlin("jvm") version "2.0.0"
    application
}

dependencies {
    implementation("net.java.dev.jna:jna:5.18.1")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.0")
}

kotlin { jvmToolchain(21) }
sourceSets.main { kotlin.srcDir("../../../../target/sdk-conformance/kotlin/uniffi") }
sourceSets.main { kotlin.srcDir("../../../../target/sdk-conformance/kotlin/runtime") }
sourceSets.main { kotlin.srcDir("../../../../target/sdk-conformance/kotlin/android") }

application { mainClass.set("ConformanceKt") }
tasks.named<JavaExec>("run") {
    jvmArgs(
        "-Xss16m",
        "-XX:+UseSerialGC",
        "-XX:-DisableExplicitGC",
        "-Djna.library.path=${project.rootDir}/../../../../target/sdk-conformance-artifacts/native",
    )
    environment("RUST_MIN_STACK", "16777216")
}

kotlin { compilerOptions { freeCompilerArgs.add("-Xjvm-default=all") } }
