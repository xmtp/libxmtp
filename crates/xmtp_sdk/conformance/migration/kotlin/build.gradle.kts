plugins {
    kotlin("jvm") version "2.0.0"
    application
}
dependencies {
    implementation("net.java.dev.jna:jna:5.18.1")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.0")
}
kotlin { jvmToolchain(21) }
sourceSets.main {
    kotlin.srcDir("../../../../../target/sdk-generated/kotlin/uniffi")
    kotlin.srcDir("../../../../../target/sdk-generated/kotlin/runtime")
    kotlin.srcDir("../../../../../target/sdk-generated/kotlin/android")
    kotlin.srcDir("../../kotlin/src/main/kotlin/android")
    kotlin.srcDir("../../kotlin/src/main/kotlin/androidx")
}
kotlin { compilerOptions { freeCompilerArgs.add("-Xjvm-default=all") } }
application { mainClass.set("MigrationConformanceKt") }
tasks.named<JavaExec>("run") {
    jvmArgs("-Xss16m", "-Djna.library.path=$rootDir/../../../../../target/sdk-artifacts/native")
    args("$rootDir/../../../../../crates/xmtp_legacy_migration/fixtures/encrypted.db3")
}
