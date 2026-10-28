package io.github.wenshiqi0.kite

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import io.github.wenshiqi0.kite.core.initLogging
import io.github.wenshiqi0.kite.ui.HostCheckScreen
import io.github.wenshiqi0.kite.ui.theme.KiteTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        initLogging()
        enableEdgeToEdge()
        setContent {
            KiteTheme {
                HostCheckScreen()
            }
        }
    }
}
