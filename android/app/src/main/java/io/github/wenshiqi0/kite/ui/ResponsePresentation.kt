package io.github.wenshiqi0.kite.ui

import io.github.wenshiqi0.kite.core.ResponsePhase

/** Attention shown to the user is separate from a nonblocking model task's activity. */
internal fun responsePresentationPhase(phase: ResponsePhase, sending: Boolean, hasQuestion: Boolean, hasApproval: Boolean): ResponsePhase = when {
    hasQuestion || hasApproval -> ResponsePhase.WAITING
    sending && phase == ResponsePhase.IDLE -> ResponsePhase.SENDING
    else -> phase
}

internal fun responseFailureDetails(phase: ResponsePhase, details: String): String? =
    details.trim().takeIf { phase == ResponsePhase.FAILED && it.isNotEmpty() }

internal fun responseIsModelCapacity(details: String): Boolean =
    details.contains("serverOverloaded", ignoreCase = true) ||
        details.contains("Selected model is at capacity", ignoreCase = true)
