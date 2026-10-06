plugins {
    kotlin("jvm") version "2.0.0"
    application
}
dependencies {
    implementation("net.java.dev.jna:jna:5.18.1")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.0")
}
kotlin { jvmToolchain(21) }
sourceSets.main { kotlin.srcDir("../../../../target/migration-generated/kotlin") }
application { mainClass.set("MigrationConformanceKt") }
tasks.named<JavaExec>("run") {
    jvmArgs("-Djna.library.path=$rootDir/../../../../target/debug")
    args("$rootDir/../../../../crates/xmtp_legacy_migration/fixtures/encrypted.db3")
}
