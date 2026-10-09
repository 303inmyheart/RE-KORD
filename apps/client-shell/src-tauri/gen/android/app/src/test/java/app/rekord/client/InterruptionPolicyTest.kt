package app.rekord.client

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Resume after another app's audio: `./gradlew :app:testDebugUnitTest` from gen/android.
 */
class InterruptionPolicyTest {
    private val ext = InterruptionPolicy.REASON_EXTERNAL

    private fun interrupted(at: Long = 1_000L): InterruptionPolicy {
        val p = InterruptionPolicy()
        p.onPageState(playing = true, pauseReason = "", now = 0L)
        p.onPageState(playing = false, pauseReason = ext, now = at)
        assertTrue(p.armed)
        return p
    }

    @Test
    fun resumesOnceTheOtherAudioHasBeenQuietForAWhile() {
        val p = interrupted()
        assertEquals(InterruptionPolicy.Decision.Wait(null), p.decide(5_000L, otherAudioActive = true, inCall = false))
        // Silence starts: wait for the quiet window (Chromium may resume on its own).
        assertEquals(InterruptionPolicy.Decision.Wait(2_500L), p.decide(60_000L, false, false))
        assertEquals(InterruptionPolicy.Decision.Wait(1_000L), p.decide(61_500L, false, false))
        assertEquals(InterruptionPolicy.Decision.Resume, p.decide(62_500L, false, false))
        // The page plays again: disarmed.
        p.onPageState(playing = true, pauseReason = "", now = 63_000L)
        assertFalse(p.armed)
        assertEquals(InterruptionPolicy.Decision.None, p.decide(70_000L, false, false))
    }

    @Test
    fun otherAudioStartingAgainRestartsTheQuietWindow() {
        val p = interrupted()
        p.decide(10_000L, false, false)
        p.decide(11_000L, otherAudioActive = true, inCall = false)
        assertEquals(InterruptionPolicy.Decision.Wait(2_500L), p.decide(12_000L, false, false))
    }

    @Test
    fun neverResumesDuringACall() {
        val p = interrupted()
        for (t in listOf(5_000L, 20_000L, 60_000L)) {
            assertEquals(InterruptionPolicy.Decision.Wait(null), p.decide(t, otherAudioActive = false, inCall = true))
        }
        p.decide(61_000L, false, false)
        assertEquals(InterruptionPolicy.Decision.Resume, p.decide(64_000L, false, false))
    }

    @Test
    fun aUserPauseIsNeverUndone() {
        // Paused by the user in the app: the page says so.
        val p = InterruptionPolicy()
        p.onPageState(true, "", 0L)
        p.onPageState(false, "user", 1_000L)
        assertFalse(p.armed)
        assertEquals(InterruptionPolicy.Decision.None, p.decide(10_000L, false, false))
        // Interrupted first, then pause from the notification / the car.
        val q = interrupted()
        q.onUserPause()
        assertEquals(InterruptionPolicy.Decision.None, q.decide(10_000L, false, false))
    }

    @Test
    fun headphonesOrBluetoothGoneDisarms() {
        val p = interrupted()
        p.onNoisy()
        assertFalse(p.armed)
        assertEquals(InterruptionPolicy.Decision.None, p.decide(10_000L, false, false))
    }

    @Test
    fun aLongInterruptionExpires() {
        val p = interrupted(at = 0L)
        val d = p.decide(31 * 60_000L, false, false)
        assertTrue(d is InterruptionPolicy.Decision.Expired)
        assertFalse(p.armed)
    }

    @Test
    fun aResumeThePageIgnoresIsRetriedAFewTimesThenDropped() {
        val p = interrupted(at = 0L)
        p.decide(1_000L, false, false)
        assertEquals(InterruptionPolicy.Decision.Resume, p.decide(4_000L, false, false))
        assertEquals(InterruptionPolicy.Decision.Wait(5_000L), p.decide(9_000L, false, false))
        assertEquals(InterruptionPolicy.Decision.Resume, p.decide(14_000L, false, false))
        assertEquals(InterruptionPolicy.Decision.Resume, p.decide(24_000L, false, false))
        assertTrue(p.decide(34_000L, false, false) is InterruptionPolicy.Decision.Expired)
    }

    @Test
    fun anExternalPauseWithoutPriorPlaybackArmsNothing() {
        val p = InterruptionPolicy()
        p.onPageState(false, ext, 0L)
        assertFalse(p.armed)
        // A position refresh while interrupted keeps the original start time.
        val q = interrupted(at = 0L)
        q.onPageState(false, ext, 29 * 60_000L)
        assertTrue(q.decide(31 * 60_000L, false, false) is InterruptionPolicy.Decision.Expired)
    }
}
