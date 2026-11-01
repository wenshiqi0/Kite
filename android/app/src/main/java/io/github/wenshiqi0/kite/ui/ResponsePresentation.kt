package io.github.wenshiqi0.kite.ui

import io.github.wenshiqi0.kite.core.ResponsePhase

/** Attention shown to the user is separate from a nonblocking model task's activity. */
internal fun responsePresentationPhase(phase: ResponsePhase, sending: Boolean, hasQuestion: Boolean, hasApproval: Boolean): ResponsePhase = when {
    hasQuestion || hasApproval -> ResponsePhase.WAITING
    sending && phase == ResponsePhase.IDLE -> ResponsePhase.SENDING
    else -> phase
}
