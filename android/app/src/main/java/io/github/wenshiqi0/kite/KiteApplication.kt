package io.github.wenshiqi0.kite

import android.app.Application
import io.github.wenshiqi0.kite.core.initLogging
import io.github.wenshiqi0.kite.data.CoreController

class KiteApplication : Application() {
    lateinit var engine: CoreController
        private set

    override fun onCreate() {
        super.onCreate()
        initLogging()
        engine = CoreController(this)
        engine.initialize()
    }
}
