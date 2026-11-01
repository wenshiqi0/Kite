package io.github.wenshiqi0.kite.ui

import org.junit.Assert.*
import org.junit.Test

class HostResourcesTest {
    @Test fun memoryBudgetsUseBinaryGiBAndRejectInvalidNumbers() {
        assertEquals(10uL * 1073741824uL, resourceBudgetBytes("10"))
        assertEquals(536870912uL, resourceBudgetBytes("0.5"))
        for (invalid in listOf("", "NaN", "Infinity", "-1", "0", "0.1", "2048", "1e400")) assertNull(invalid, resourceBudgetBytes(invalid))
    }
    @Test fun byteLabelsKeepHumanSizedUnitsAndStableDecimals() {
        assertEquals("10.0 GiB", resourceBytes(10uL * 1073741824uL))
        assertEquals("512 MiB", resourceBytes(512uL * 1024uL * 1024uL))
        assertEquals("0 MiB", resourceBytes(0uL))
        assertFalse(canStartResourceAgent(null))
    }
}
