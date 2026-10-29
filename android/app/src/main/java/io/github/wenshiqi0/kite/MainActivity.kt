package io.github.wenshiqi0.kite

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.lifecycle.ViewModelProvider
import io.github.wenshiqi0.kite.ui.HostCheckScreen
import io.github.wenshiqi0.kite.ui.HostCheckViewModel
import io.github.wenshiqi0.kite.ui.theme.KiteTheme

class MainActivity : ComponentActivity() {
    private lateinit var model: HostCheckViewModel
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        model = ViewModelProvider(this)[HostCheckViewModel::class.java]
        setContent { KiteTheme { HostCheckScreen(model) } }
    }
    override fun onStart() { super.onStart(); if (::model.isInitialized) model.onForeground() }
    override fun onStop() { if (!isChangingConfigurations && ::model.isInitialized) model.onBackground(); super.onStop() }
}
