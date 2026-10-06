package app.rekord.client

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.os.PowerManager
import android.webkit.JavascriptInterface
import android.webkit.WebView
import org.json.JSONObject

/**
 * The track now playing, as reported by the player in the WebView.
 *
 * The audio plays inside the page: nothing is played here, we just tell the
 * system what is playing so it can draw the notification and the lock
 * screen.
 */
data class NowPlaying(
    val title: String,
    val artist: String,
    val album: String,
    val artworkUrl: String?,
    val playing: Boolean,
    val durationMs: Long,
    val positionMs: Long,
) {
    companion object {
        fun fromJson(raw: String): NowPlaying? {
            return try {
                val o = JSONObject(raw)
                val title = o.optString("title").trim()
                if (title.isEmpty()) return null
                NowPlaying(
                    title = title,
                    artist = o.optString("artist").trim(),
                    album = o.optString("album").trim(),
                    artworkUrl = o.optString("artworkUrl").trim().ifEmpty { null },
                    playing = o.optBoolean("playing", false),
                    durationMs = o.optLong("durationMs", 0L).coerceAtLeast(0L),
                    positionMs = o.optLong("positionMs", 0L).coerceAtLeast(0L),
                )
            } catch (e: Exception) {
                Logger.warn("RekordMedia: unreadable state: ${e.message}")
                null
            }
        }
    }
}

/**
 * Notification commands travel back to the player along the path the client
 * already has for native shells: a DOM event, with the same action names as the
 * Media Session (see `src/lib/mediaSession.ts`).
 *
 * With the screen off, delivery is not guaranteed: the WebView may be suspended,
 * the CPU may fall back asleep before the script runs, the page may still be
 * loading. As in 5.0: a wake lock of a few seconds, the WebView woken up, and a
 * script that answers `true` only if the player is there
 * (`window.__rekordNativeMediaReady`, set by `nativeMedia.ts` on the first state
 * sent). Otherwise it retries a few times, then gives up.
 */
object RekordMediaBridge {
    @Volatile
    var webView: WebView? = null

    private const val WAKE_MS = 5_000L
    private const val RETRY_MS = 250L
    private const val MAX_ATTEMPTS = 12

    private val main = Handler(Looper.getMainLooper())
    private var wakeLock: PowerManager.WakeLock? = null

    /** Only one command in flight: a "play" followed by "pause" counts as "pause". */
    private var pending: Pair<String, Double?>? = null
    private var attempts = 0
    private val retry = Runnable { deliver() }

    fun attach(context: Context) {
        if (wakeLock != null) return
        val pm = context.getSystemService(Context.POWER_SERVICE) as? PowerManager ?: return
        wakeLock = pm.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "Rekord:MediaCommand").apply {
            setReferenceCounted(false)
        }
    }

    fun send(action: String, value: Double? = null) {
        main.post {
            pending = action to value
            attempts = 0
            main.removeCallbacks(retry)
            try {
                wakeLock?.acquire(WAKE_MS)
            } catch (e: Exception) {
                Logger.warn("RekordMedia: wake lock unavailable: ${e.message}")
            }
            deliver()
        }
    }

    private fun deliver() {
        val command = pending ?: return
        val view = webView
        if (view == null) {
            scheduleRetry()
            return
        }
        val (action, value) = command
        val detail = if (value == null) {
            "{action:'$action'}"
        } else {
            "{action:'$action',value:$value}"
        }
        val script = "(function(){if(!window.__rekordNativeMediaReady)return false;" +
            "window.dispatchEvent(new CustomEvent('rekord:media-action',{detail:$detail}));" +
            "return true})()"
        // With the app in the background and the music paused, the WebView is suspended:
        // a play from the notification would talk to a sleeping player. Wake it up
        // first, then tell it what to do.
        view.onResume()
        view.resumeTimers()
        view.evaluateJavascript(script) { result ->
            if (pending !== command) return@evaluateJavascript
            if (result == "true") {
                done()
            } else {
                scheduleRetry()
            }
        }
    }

    private fun scheduleRetry() {
        attempts += 1
        if (attempts >= MAX_ATTEMPTS) {
            Logger.warn("RekordMedia: command ${pending?.first} not delivered")
            done()
            return
        }
        main.removeCallbacks(retry)
        main.postDelayed(retry, RETRY_MS)
    }

    private fun done() {
        pending = null
        attempts = 0
        main.removeCallbacks(retry)
        try {
            if (wakeLock?.isHeld == true) wakeLock?.release()
        } catch (e: Exception) {
            Logger.warn("RekordMedia: wake lock not released: ${e.message}")
        }
    }
}

/**
 * Surface exposed to the page as `window.RekordMediaNative`. The WebView
 * loads only our local bundle, so no third-party page can
 * call it.
 */
class RekordMedia(private val activity: MainActivity) {
    @JavascriptInterface
    fun update(json: String) {
        val state = NowPlaying.fromJson(json) ?: return
        activity.runOnUiThread {
            // The permission is requested on the first track, not at startup: first
            // there is something to show, then we ask to be allowed to show it.
            if (state.playing) activity.ensureNotificationPermission()
            RekordMediaService.publish(activity, state)
        }
    }

    @JavascriptInterface
    fun stop() {
        activity.runOnUiThread { RekordMediaService.stop(activity) }
    }
}
