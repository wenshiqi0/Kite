package io.github.wenshiqi0.kite.ui

import io.github.wenshiqi0.kite.core.ChatSkillInfo

internal val ChatSkillInfo.presentationName: String
    get() = displayName?.takeIf { it.isNotBlank() } ?: name

internal val ChatSkillInfo.presentationDescription: String
    get() = shortDescription?.takeIf { it.isNotBlank() } ?: description

internal fun ChatSkillInfo.matchesSearch(query: String): Boolean =
    listOfNotNull(displayName, shortDescription, name, description).any {
        it.contains(query, ignoreCase = true)
    }
