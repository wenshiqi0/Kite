package io.github.wenshiqi0.kite.ui

import org.junit.Assert.*
import org.junit.Test

class CreditExpiryTest {
    @Test fun countdownUsesTheEarliestTimestampWithoutChangingCardCount() {
        val now = 1_790_000_000L
        assertEquals(CreditTimeLeft(2, 3, 4, false), creditTimeLeft(now + 2*86400 + 3*3600 + 4*60 + 59, now))
        assertEquals(CreditTimeLeft(0, 0, 0, false), creditTimeLeft(now + 59, now))
        assertEquals(CreditTimeLeft(0, 0, 0, true), creditTimeLeft(now, now))
        assertEquals(CreditTimeLeft(0, 0, 0, true), creditTimeLeft(now - 1, now))
    }
}
