package app.rekord.client

/**
 * Decides when playback stopped by another app may resume on its own.
 *
 * Audio focus belongs to the WebView (see [RekordMediaService]). Chromium already
 * handles the short cases: on a transient loss (a call, a voice note, a navigation
 * prompt) it suspends the `<audio>` element and resumes it when focus comes back.
 * On a permanent loss (another music app, a video) it pauses and abandons focus, so
 * nothing ever resumes it. This policy covers that second case, and backs up the
 * first one on devices where the resume never comes.
 *
 * Inputs:
 * - what the page reports: playing, or paused with a reason. Only `external` (a pause
 *   the player did not ask for: Chromium reacting to a focus loss) arms the resume;
 * - user commands (pause from the notification, the headset, the car) and the
 *   "becoming noisy" broadcast (headphones or Bluetooth gone): both disarm it;
 * - when checked: whether other audio is playing and whether a call is up.
 *
 * Plain Kotlin, no Android types: `InterruptionPolicyTest` covers it on the JVM.
 */
class InterruptionPolicy(
    /** Longer interruptions are a change of plans: no resume after this. */
    private val maxInterruptionMs: Long = 30 * 60_000L,
    /** The other audio must have been silent this long (lets Chromium resume first). */
    private val quietMs: Long = 2_500L,
    /** A resume the page did not act on is sent again after this. */
    private val resendMs: Long = 10_000L,
    private val maxResumes: Int = 3,
) {
    enum class Phase { IDLE, PLAYING, PAUSED, INTERRUPTED }

    sealed class Decision {
        /** Nothing armed. */
        object None : Decision()

        /** Armed, but not yet: check again in [recheckMs] (null: on the next event). */
        data class Wait(val recheckMs: Long?) : Decision()

        /** Send "play" to the page. */
        object Resume : Decision()

        /** The interruption lasted too long, or the page never resumed: disarmed. */
        data class Expired(val reason: String) : Decision()
    }

    var phase: Phase = Phase.IDLE
        private set

    private var interruptedAt = 0L
    private var quietSince: Long? = null
    private var resumes = 0
    private var lastResumeAt = 0L

    /** True while an automatic resume may still happen. */
    val armed: Boolean
        get() = phase == Phase.INTERRUPTED

    /** Returns a short description of the transition, or null if nothing changed. */
    fun onPageState(playing: Boolean, pauseReason: String, now: Long): String? {
        val before = phase
        when {
            playing -> {
                phase = Phase.PLAYING
                resumes = 0
            }
            pauseReason == REASON_EXTERNAL -> {
                // Only a track that was playing can be interrupted; a later "paused,
                // external" (position refresh) keeps the original start time.
                if (phase == Phase.PLAYING) {
                    phase = Phase.INTERRUPTED
                    interruptedAt = now
                    quietSince = null
                    resumes = 0
                    lastResumeAt = 0L
                }
            }
            else -> phase = Phase.PAUSED
        }
        return if (before != phase) "$before -> $phase" else null
    }

    /** Pause/stop from the user (notification, headset, car): never resume after it. */
    fun onUserPause(): String? = disarm("user pause")

    /** Headphones unplugged / Bluetooth disconnected. */
    fun onNoisy(): String? = disarm("audio output disconnected")

    /** Playback session over (queue cleared, notification dismissed). */
    fun reset() {
        phase = Phase.IDLE
        quietSince = null
        resumes = 0
    }

    private fun disarm(why: String): String? {
        if (phase != Phase.INTERRUPTED && phase != Phase.PLAYING) return null
        val before = phase
        phase = Phase.PAUSED
        quietSince = null
        return "$before -> PAUSED ($why)"
    }

    fun decide(now: Long, otherAudioActive: Boolean, inCall: Boolean): Decision {
        if (phase != Phase.INTERRUPTED) return Decision.None
        if (now - interruptedAt > maxInterruptionMs) {
            phase = Phase.PAUSED
            return Decision.Expired("interrupted for more than ${maxInterruptionMs / 60_000} min")
        }
        if (otherAudioActive || inCall) {
            quietSince = null
            return Decision.Wait(null)
        }
        val quiet = quietSince ?: now.also { quietSince = it }
        val waited = now - quiet
        if (waited < quietMs) return Decision.Wait(quietMs - waited)
        if (lastResumeAt != 0L && now - lastResumeAt < resendMs) {
            return Decision.Wait(resendMs - (now - lastResumeAt))
        }
        if (resumes >= maxResumes) {
            phase = Phase.PAUSED
            return Decision.Expired("the page did not resume after $maxResumes attempts")
        }
        resumes += 1
        lastResumeAt = now
        return Decision.Resume
    }

    companion object {
        /** `pauseReason` the page sends for a pause it did not ask for. */
        const val REASON_EXTERNAL = "external"
    }
}
