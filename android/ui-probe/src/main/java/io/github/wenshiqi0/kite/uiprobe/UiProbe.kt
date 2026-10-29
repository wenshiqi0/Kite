package io.github.wenshiqi0.kite.uiprobe

import android.app.Activity
import android.app.Instrumentation
import android.accessibilityservice.AccessibilityServiceInfo
import android.os.Bundle
import android.os.SystemClock
import android.view.accessibility.AccessibilityNodeInfo

/** Runs in its own test-only package so it does not restart the streaming app. */
class UiProbe : Instrumentation() {
    private var text: String? = null
    override fun onCreate(arguments: Bundle?) { super.onCreate(arguments); text = arguments?.getString("text"); start() }
    override fun onStart() {
        try {
            val value = checkNotNull(text) { "text argument required" }
            val info = uiAutomation.serviceInfo
            info.flags = info.flags or AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS or AccessibilityServiceInfo.FLAG_INCLUDE_NOT_IMPORTANT_VIEWS
            uiAutomation.serviceInfo = info
            val deadline = SystemClock.uptimeMillis() + 3000
            var candidate = uiAutomation.rootInActiveWindow
            while (candidate == null && SystemClock.uptimeMillis() < deadline) { SystemClock.sleep(50); candidate = uiAutomation.rootInActiveWindow }
            val root = checkNotNull(candidate) { "no active root" }
            check(root.packageName?.toString() == "io.github.wenshiqi0.kite") { "wrong root package: ${root.packageName}" }
            val nodes = mutableListOf<AccessibilityNodeInfo>()
            fun visit(node: AccessibilityNodeInfo) {
                if (node.className?.toString() == "android.widget.EditText" && node.isEnabled) nodes.add(node)
                for (index in 0 until node.childCount) node.getChild(index)?.let(::visit)
            }
            visit(root)
            check(nodes.size == 1) { "one visible text composer required, found ${nodes.size}" }
            nodes.single().performAction(AccessibilityNodeInfo.ACTION_FOCUS)
            val args = Bundle().apply { putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, value) }
            check(nodes.single().performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, args)) { "set-text rejected" }
            finish(Activity.RESULT_OK, Bundle().apply { putBoolean("text_set", true) })
        } catch (e: Exception) {
            finish(Activity.RESULT_CANCELED, Bundle().apply { putString("failure", "${e.javaClass.simpleName}: ${e.message}") })
        }
    }
}
