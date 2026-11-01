package io.github.wenshiqi0.kite.ui

import androidx.annotation.StringRes
import io.github.wenshiqi0.kite.R

/** Every row has an explicit state, including saved and disconnected sessions. */
enum class SessionStatus(@param:StringRes val label: Int) {
    Running(R.string.session_running),
    Question(R.string.session_question),
    Attention(R.string.session_attention),
    Starting(R.string.agent_starting_short),
    Paused(R.string.agent_paused),
    Completed(R.string.agent_done),
    Idle(R.string.agent_idle),
    Failed(R.string.session_failed),
    Saved(R.string.session_saved),
    Disconnected(R.string.session_disconnected),
    Unknown(R.string.agent_unknown)
}

fun sessionStatus(status: String, connected: Boolean): SessionStatus = if (!connected) SessionStatus.Disconnected else when (status) {
    "working" -> SessionStatus.Running
    "question" -> SessionStatus.Question
    "blocked" -> SessionStatus.Attention
    "starting" -> SessionStatus.Starting
    "paused" -> SessionStatus.Paused
    "done", "completed" -> SessionStatus.Completed
    "idle" -> SessionStatus.Idle
    "failed", "error" -> SessionStatus.Failed
    "saved" -> SessionStatus.Saved
    else -> SessionStatus.Unknown
}
