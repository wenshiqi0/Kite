// Optional device-side accessibility fixture; never a production dependency.
plugins { alias(libs.plugins.android.application) }
android {
    namespace = "io.github.wenshiqi0.kite.uiprobe"
    compileSdk { version = release(37) }
    defaultConfig {
        applicationId = "io.github.wenshiqi0.kite.uiprobe"
        minSdk { version = release(26) }
        targetSdk { version = release(36) }
        versionCode = 1
        versionName = "1"
    }
}
