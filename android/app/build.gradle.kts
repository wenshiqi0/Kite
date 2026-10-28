import org.jetbrains.kotlin.gradle.dsl.JvmTarget
import javax.inject.Inject

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

val minSdkVersion = 26

android {
    namespace = "io.github.wenshiqi0.kite"
    compileSdk {
        version = release(36)
    }
    ndkVersion = "30.0.16248370"

    defaultConfig {
        applicationId = "io.github.wenshiqi0.kite"
        minSdk {
            version = release(minSdkVersion)
        }
        targetSdk {
            version = release(36)
        }
        versionCode = 1
        versionName = "0.1.0"
    }

    buildFeatures {
        compose = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_17
    }
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.material3)
    implementation(libs.kotlinx.coroutines.android)
    // The AAR carries JNA's native dispatch library for each Android ABI.
    implementation(libs.jna) { artifact { type = "aar" } }
}

// --- Rust core -------------------------------------------------------------
// cargo-ndk builds kite-ffi into libkite_ffi.so per ABI, then uniffi-bindgen
// reads that library and generates the Kotlin bindings.

/** Builds the Rust core with cargo-ndk. Cargo tracks its own inputs, so this always runs. */
abstract class CargoNdkBuild : DefaultTask() {
    @get:Inject abstract val execOperations: ExecOperations
    @get:Input abstract val cargo: Property<String>
    @get:Input abstract val abis: ListProperty<String>
    @get:Input abstract val platform: Property<Int>
    @get:Internal abstract val workspaceDir: DirectoryProperty
    @get:Internal abstract val ndkDir: DirectoryProperty
    @get:OutputDirectory abstract val outputDir: DirectoryProperty

    init {
        outputs.upToDateWhen { false }
    }

    @TaskAction
    fun build() {
        val args = buildList {
            add(cargo.get())
            add("ndk")
            abis.get().forEach { add("-t"); add(it) }
            addAll(listOf("--platform", platform.get().toString()))
            addAll(listOf("-o", outputDir.get().asFile.path))
            addAll(listOf("build", "--release", "-p", "kite-ffi"))
        }
        execOperations.exec {
            workingDir(workspaceDir.get().asFile)
            environment("ANDROID_NDK_HOME", ndkDir.get().asFile.path)
            commandLine(args)
        }
    }
}

/** Generates Kotlin bindings from the built library's UniFFI metadata. */
abstract class UniffiBindgen : DefaultTask() {
    @get:Inject abstract val execOperations: ExecOperations
    @get:Input abstract val cargo: Property<String>
    @get:Internal abstract val workspaceDir: DirectoryProperty
    @get:InputDirectory abstract val libraryDir: DirectoryProperty
    @get:OutputDirectory abstract val outputDir: DirectoryProperty

    @TaskAction
    fun generate() {
        val library = libraryDir.get().asFile.walk().first { it.name == "libkite_ffi.so" }
        val out = outputDir.get().asFile
        out.deleteRecursively()
        execOperations.exec {
            workingDir(workspaceDir.get().asFile)
            commandLine(
                cargo.get(), "run", "--quiet", "-p", "uniffi-bindgen", "--",
                "generate", "--library", library.path,
                "--language", "kotlin", "--no-format", "--out-dir", out.path,
            )
        }
    }
}

val cargoPath = providers.environmentVariable("CARGO")
    .orElse(providers.systemProperty("user.home").map { "$it/.cargo/bin/cargo" })
val rustWorkspaceDir = rootProject.layout.projectDirectory.dir("..")

val cargoNdkBuild = tasks.register<CargoNdkBuild>("cargoNdkBuild") {
    cargo.set(cargoPath)
    abis.set(providers.gradleProperty("kite.rustAbis").map { it.split(',').map(String::trim) })
    platform.set(minSdkVersion)
    workspaceDir.set(rustWorkspaceDir)
    ndkDir.set(androidComponents.sdkComponents.ndkDirectory)
    outputDir.set(layout.buildDirectory.dir("rust/jniLibs"))
}

val uniffiBindgen = tasks.register<UniffiBindgen>("uniffiBindgen") {
    cargo.set(cargoPath)
    workspaceDir.set(rustWorkspaceDir)
    libraryDir.set(cargoNdkBuild.flatMap { it.outputDir })
    outputDir.set(layout.buildDirectory.dir("generated/uniffi/kotlin"))
}

androidComponents {
    onVariants { variant ->
        variant.sources.jniLibs?.addGeneratedSourceDirectory(cargoNdkBuild, CargoNdkBuild::outputDir)
        variant.sources.kotlin?.addGeneratedSourceDirectory(uniffiBindgen, UniffiBindgen::outputDir)
    }
}
