package app.rekord.client

import android.app.Activity
import android.app.Dialog
import android.content.Context
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.webkit.JavascriptInterface
import androidx.mediarouter.app.MediaRouteChooserDialog
import androidx.mediarouter.media.MediaRouter
import com.google.android.gms.cast.CastMediaControlIntent
import com.google.android.gms.cast.MediaInfo
import com.google.android.gms.cast.MediaLoadRequestData
import com.google.android.gms.cast.MediaMetadata
import com.google.android.gms.cast.MediaSeekOptions
import com.google.android.gms.cast.MediaStatus
import com.google.android.gms.cast.framework.CastContext
import com.google.android.gms.cast.framework.CastOptions
import com.google.android.gms.cast.framework.CastSession
import com.google.android.gms.cast.framework.CastState
import com.google.android.gms.cast.framework.CastStateListener
import com.google.android.gms.cast.framework.OptionsProvider
import com.google.android.gms.cast.framework.SessionManagerListener
import com.google.android.gms.cast.framework.SessionProvider
import com.google.android.gms.cast.framework.media.CastMediaOptions
import com.google.android.gms.cast.framework.media.RemoteMediaClient
import com.google.android.gms.common.ConnectionResult
import com.google.android.gms.common.GoogleApiAvailability
import com.google.android.gms.common.images.WebImage
import org.json.JSONObject
import java.lang.ref.WeakReference
import java.util.concurrent.Executors

/**
 * Cast options (read by Play Services through the meta-data in the manifest):
 * Default Media Receiver, as in 5.0 and as the web sender.
 *
 * The Cast SDK's notification and MediaSession are disabled: [RekordMediaService]
 * already draws the notification with the state the page mirrors from the receiver,
 * and two notifications for the same track would be a duplicate.
 */
class RekordCastOptionsProvider : OptionsProvider {
    override fun getCastOptions(context: Context): CastOptions {
        val media = CastMediaOptions.Builder()
            .setNotificationOptions(null)
            .setMediaSessionEnabled(false)
            .build()
        return CastOptions.Builder()
            .setReceiverApplicationId(CastMediaControlIntent.DEFAULT_MEDIA_RECEIVER_APPLICATION_ID)
            .setCastMediaOptions(media)
            .setResumeSavedSession(true)
            .setStopReceiverApplicationWhenEndingSession(true)
            .build()
    }

    override fun getAdditionalSessionProviders(context: Context): List<SessionProvider>? = null
}

/**
 * Native Google Cast (port of 5.0's `RekordCastManager`).
 *
 * 5.0 let the page play muted and reloaded every Media Session state onto the
 * receiver. Here the page decides everything: `src/lib/cast/androidCast.ts`
 * is a `CastBackend` like the web sender, calls the methods of [RekordCastJs] and
 * receives the receiver's state as a `rekord:cast` DOM event — the same path
 * [RekordMediaBridge] uses for notification commands.
 *
 * Event details (`detail`):
 *  - `{type:"status", status:{...}}`: see [statusJson], ~1 Hz during playback;
 *  - `{type:"session", event:"cancelled"|"startFailed"|"unavailable", code?}`;
 *  - `{type:"result", id, ok, error?}`: outcome of a `load`.
 *
 * The whole Cast SDK lives on the main thread: the bridge methods arrive on the
 * JavaBridge thread and only post.
 */
object RekordCast {
    private const val EVENT = "rekord:cast"
    /** Volume key step on the Cast device (0..1). */
    private const val VOLUME_STEP = 0.05

    private val main = Handler(Looper.getMainLooper())

    private var appContext: Context? = null
    private var castContext: CastContext? = null
    private var initStarted = false
    private var ready = false
    private var supported = false
    private var unsupportedReason: String? = null

    private var session: CastSession? = null
    private var client: RemoteMediaClient? = null
    /** Session starting or resuming: a closed picker does not mean "cancelled". */
    private var starting = false

    private var activity: WeakReference<Activity>? = null
    private var chooser: Dialog? = null

    private var lastSent = ""

    /** Latest status, read synchronously by the page with `getStatus()`. */
    @Volatile
    var latestStatus: String = "{\"ready\":false,\"supported\":false}"
        private set

    /** Used by MainActivity (WebView kept awake, volume keys) and by the notification. */
    @Volatile
    var isConnected = false
        private set

    private val castStateListener = CastStateListener { emitStatus() }

    private val mediaCallback = object : RemoteMediaClient.Callback() {
        override fun onStatusUpdated() = emitStatus()
        override fun onMetadataUpdated() = emitStatus()
    }

    private val progressListener = RemoteMediaClient.ProgressListener { _, _ -> emitStatus() }

    private val sessionListener = object : SessionManagerListener<CastSession> {
        override fun onSessionStarting(s: CastSession) {
            starting = true
            emitStatus()
        }

        override fun onSessionStarted(s: CastSession, sessionId: String) {
            starting = false
            attach(s)
        }

        override fun onSessionStartFailed(s: CastSession, error: Int) {
            starting = false
            detach()
            dispatch(JSONObject().put("type", "session").put("event", "startFailed").put("code", error))
        }

        override fun onSessionEnding(s: CastSession) = emitStatus()

        override fun onSessionEnded(s: CastSession, error: Int) {
            starting = false
            detach()
        }

        override fun onSessionResuming(s: CastSession, sessionId: String) {
            starting = true
            emitStatus()
        }

        override fun onSessionResumed(s: CastSession, wasSuspended: Boolean) {
            starting = false
            attach(s)
        }

        override fun onSessionResumeFailed(s: CastSession, error: Int) {
            starting = false
            detach()
        }

        override fun onSessionSuspended(s: CastSession, reason: Int) = emitStatus()
    }

    /**
     * Call on the main thread (MainActivity.onCreate). Without Play
     * Services, or with a version the Cast SDK doesn't accept, Cast is
     * simply unavailable: the button on the page doesn't appear.
     */
    fun init(context: Context) {
        if (initStarted) return
        initStarted = true
        appContext = context.applicationContext
        val gms = try {
            GoogleApiAvailability.getInstance().isGooglePlayServicesAvailable(context)
        } catch (e: Exception) {
            ConnectionResult.SERVICE_MISSING
        }
        if (gms != ConnectionResult.SUCCESS) {
            finishInit(null, "play-services-$gms")
            return
        }
        val executor = Executors.newSingleThreadExecutor()
        try {
            CastContext.getSharedInstance(context.applicationContext, executor)
                .addOnSuccessListener { ctx ->
                    executor.shutdown()
                    finishInit(ctx, null)
                }
                .addOnFailureListener { e ->
                    executor.shutdown()
                    Logger.warn("RekordCast: CastContext unavailable: ${e.message}")
                    finishInit(null, "cast-context")
                }
        } catch (e: Exception) {
            executor.shutdown()
            Logger.warn("RekordCast: Cast SDK unavailable: ${e.message}")
            finishInit(null, "cast-sdk")
        }
    }

    private fun finishInit(ctx: CastContext?, reason: String?) {
        ready = true
        if (ctx == null) {
            supported = false
            unsupportedReason = reason
            emitStatus()
            return
        }
        try {
            ctx.addCastStateListener(castStateListener)
            ctx.sessionManager.addSessionManagerListener(sessionListener, CastSession::class.java)
            castContext = ctx
            supported = true
            unsupportedReason = null
            // Session resumed at startup (app reopened while the Chromecast was playing).
            ctx.sessionManager.currentCastSession?.takeIf { it.isConnected }?.let { attach(it) }
        } catch (e: Exception) {
            Logger.warn("RekordCast: initialization failed: ${e.message}")
            castContext = null
            supported = false
            unsupportedReason = "cast-init"
        }
        emitStatus()
    }

    private fun attach(s: CastSession) {
        if (session !== s) {
            releaseClient()
            session = s
        }
        val c = s.remoteMediaClient
        if (c != null && c !== client) {
            client = c
            c.registerCallback(mediaCallback)
            c.addProgressListener(progressListener, 1_000L)
        }
        isConnected = true
        RekordMediaService.setCastTarget(s.castDevice?.friendlyName ?: "Cast")
        emitStatus()
    }

    private fun detach() {
        releaseClient()
        session = null
        val was = isConnected
        isConnected = false
        if (was) RekordMediaService.setCastTarget(null)
        emitStatus()
    }

    private fun releaseClient() {
        client?.let {
            it.unregisterCallback(mediaCallback)
            it.removeProgressListener(progressListener)
        }
        client = null
    }

    // ── Status to the page ────────────────────────────────────────────────

    private fun castStateName(state: Int): String = when (state) {
        CastState.NO_DEVICES_AVAILABLE -> "NO_DEVICES_AVAILABLE"
        CastState.NOT_CONNECTED -> "NOT_CONNECTED"
        CastState.CONNECTING -> "CONNECTING"
        CastState.CONNECTED -> "CONNECTED"
        else -> "UNKNOWN"
    }

    private fun playerStateName(state: Int): String? = when (state) {
        MediaStatus.PLAYER_STATE_IDLE -> "IDLE"
        MediaStatus.PLAYER_STATE_PLAYING -> "PLAYING"
        MediaStatus.PLAYER_STATE_PAUSED -> "PAUSED"
        MediaStatus.PLAYER_STATE_BUFFERING -> "BUFFERING"
        MediaStatus.PLAYER_STATE_LOADING -> "LOADING"
        else -> null
    }

    private fun idleReasonName(reason: Int): String? = when (reason) {
        MediaStatus.IDLE_REASON_FINISHED -> "FINISHED"
        MediaStatus.IDLE_REASON_CANCELED -> "CANCELLED"
        MediaStatus.IDLE_REASON_INTERRUPTED -> "INTERRUPTED"
        MediaStatus.IDLE_REASON_ERROR -> "ERROR"
        else -> null
    }

    /** Raw status: `androidCastStatus.ts` converts it into a `CastStatus`. */
    private fun statusJson(): JSONObject {
        val o = JSONObject()
            .put("ready", ready)
            .put("supported", supported)
        unsupportedReason?.let { o.put("reason", it) }
        val ctx = castContext ?: return o.put("castState", "NO_DEVICES_AVAILABLE")
        try {
            o.put("castState", castStateName(ctx.castState))
            val s = session
            o.put("deviceName", s?.castDevice?.friendlyName ?: JSONObject.NULL)
            val c = client
            if (s != null && c != null) {
                o.put("playerState", playerStateName(c.playerState) ?: JSONObject.NULL)
                o.put("idleReason", idleReasonName(c.idleReason) ?: JSONObject.NULL)
                o.put("mediaLoaded", c.hasMediaSession())
                o.put("positionMs", c.approximateStreamPosition.coerceAtLeast(0L))
                o.put("durationMs", c.streamDuration.coerceAtLeast(0L))
                o.put("contentId", c.mediaInfo?.contentId ?: JSONObject.NULL)
                try {
                    o.put("volume", s.volume)
                } catch (e: Exception) {
                    /* Session closing: the volume can no longer be read. */
                }
            }
        } catch (e: Exception) {
            Logger.warn("RekordCast: unreadable status: ${e.message}")
        }
        return o
    }

    private fun emitStatus() {
        val status = statusJson()
        val json = status.toString()
        latestStatus = json
        if (json == lastSent) return
        lastSent = json
        dispatch(JSONObject().put("type", "status").put("status", status))
    }

    private fun dispatch(detail: JSONObject) {
        val view = RekordMediaBridge.webView ?: return
        // JSONObject produces a valid JS literal: it is pasted in as is.
        val script = "window.dispatchEvent(new CustomEvent('$EVENT',{detail:$detail}))"
        main.post {
            try {
                view.evaluateJavascript(script, null)
            } catch (e: Exception) {
                Logger.warn("RekordCast: event not delivered: ${e.message}")
            }
        }
    }

    private fun result(id: Long, ok: Boolean, error: String? = null) {
        val o = JSONObject().put("type", "result").put("id", id).put("ok", ok)
        if (error != null) o.put("error", error)
        dispatch(o)
    }

    // ── Commands from the page (always on the main thread) ────────────────

    fun bindActivity(a: Activity?) {
        activity = a?.let { WeakReference(it) }
        if (a == null) {
            chooser?.dismiss()
            chooser = null
        }
    }

    /**
     * androidx.mediarouter device picker. Choosing a device selects the route,
     * and the Cast SDK starts the session on its own; closing the picker without
     * choosing is reported to the page as "cancelled".
     */
    fun requestSession() {
        val ctx = castContext
        val a = activity?.get()
        if (ctx == null || a == null || a.isFinishing) {
            dispatch(JSONObject().put("type", "session").put("event", "unavailable"))
            return
        }
        if (isConnected) {
            emitStatus()
            return
        }
        if (chooser?.isShowing == true) return
        try {
            val selector = ctx.mergedSelector ?: throw IllegalStateException("no selector")
            val dialog = MediaRouteChooserDialog(a)
            dialog.routeSelector = selector
            dialog.setOnDismissListener {
                chooser = null
                // Choosing a route closes the picker and the session starts a moment
                // later: check a little afterwards whether it really started.
                main.postDelayed({
                    val router = appContext?.let { MediaRouter.getInstance(it) }
                    val route = router?.selectedRoute
                    val picked = route != null && !route.isDefault && route.matchesSelector(selector)
                    if (!starting && !isConnected && !picked) {
                        dispatch(JSONObject().put("type", "session").put("event", "cancelled"))
                    }
                    emitStatus()
                }, 400L)
            }
            chooser = dialog
            dialog.show()
        } catch (e: Exception) {
            Logger.warn("RekordCast: picker unavailable: ${e.message}")
            chooser = null
            dispatch(JSONObject().put("type", "session").put("event", "unavailable"))
        }
    }

    fun endSession(stopReceiver: Boolean) {
        try {
            castContext?.sessionManager?.endCurrentSession(stopReceiver)
        } catch (e: Exception) {
            Logger.warn("RekordCast: ending session: ${e.message}")
        }
    }

    fun load(raw: String) {
        val o = try {
            JSONObject(raw)
        } catch (e: Exception) {
            return
        }
        val id = o.optLong("id", 0L)
        val c = client
        if (c == null || session?.isConnected != true) {
            result(id, false, "cast-no-session")
            return
        }
        val url = o.optString("url").trim()
        if (url.isEmpty()) {
            result(id, false, "cast-no-url")
            return
        }
        try {
            val metadata = MediaMetadata(MediaMetadata.MEDIA_TYPE_MUSIC_TRACK).apply {
                putString(MediaMetadata.KEY_TITLE, o.optString("title"))
                putString(MediaMetadata.KEY_ARTIST, o.optString("artist"))
                putString(MediaMetadata.KEY_ALBUM_TITLE, o.optString("album"))
                val cover = if (o.isNull("coverUrl")) "" else o.optString("coverUrl").trim()
                if (cover.isNotEmpty()) addImage(WebImage(Uri.parse(cover)))
            }
            val info = MediaInfo.Builder(url)
                .setStreamType(MediaInfo.STREAM_TYPE_BUFFERED)
                .setContentType(o.optString("contentType").ifBlank { "audio/mpeg" })
                .setMetadata(metadata)
                .build()
            val startMs = (o.optDouble("startTime", 0.0).takeIf { it.isFinite() } ?: 0.0)
                .coerceAtLeast(0.0) * 1000.0
            val request = MediaLoadRequestData.Builder()
                .setMediaInfo(info)
                .setAutoplay(o.optBoolean("autoplay", true))
                .setCurrentTime(startMs.toLong())
                .build()
            c.load(request).setResultCallback { r ->
                val status = r.status
                if (status.isSuccess) {
                    result(id, true)
                } else {
                    result(id, false, status.statusMessage ?: "cast-load-${status.statusCode}")
                }
                emitStatus()
            }
        } catch (e: Exception) {
            result(id, false, e.message ?: "cast-load")
        }
    }

    fun play() {
        client?.play()
    }

    fun pause() {
        client?.pause()
    }

    fun stop() {
        client?.stop()
    }

    fun seek(seconds: Double) {
        if (!seconds.isFinite()) return
        val ms = (seconds.coerceAtLeast(0.0) * 1000.0).toLong()
        client?.seek(MediaSeekOptions.Builder().setPosition(ms).build())
    }

    /** Device volume (0..1), for the volume keys and the notification. */
    fun volume(): Double = try {
        session?.volume ?: 0.0
    } catch (e: Exception) {
        0.0
    }

    fun setVolume(level: Double) {
        try {
            session?.volume = level.coerceIn(0.0, 1.0)
        } catch (e: Exception) {
            Logger.warn("RekordCast: volume not set: ${e.message}")
        }
    }

    fun adjustVolume(direction: Int) {
        if (direction == 0) return
        setVolume(volume() + direction * VOLUME_STEP)
    }
}

/**
 * Surface exposed to the page as `window.RekordCastNative` (the name looked up by
 * `src/lib/cast/androidCast.ts`). As with [RekordMedia], the WebView loads only
 * our bundle.
 */
class RekordCastJs {
    private val main = Handler(Looper.getMainLooper())

    @JavascriptInterface
    fun getStatus(): String = RekordCast.latestStatus

    @JavascriptInterface
    fun requestSession() {
        main.post { RekordCast.requestSession() }
    }

    @JavascriptInterface
    fun endSession(stopReceiver: Boolean) {
        main.post { RekordCast.endSession(stopReceiver) }
    }

    @JavascriptInterface
    fun load(json: String) {
        main.post { RekordCast.load(json) }
    }

    @JavascriptInterface
    fun play() {
        main.post { RekordCast.play() }
    }

    @JavascriptInterface
    fun pause() {
        main.post { RekordCast.pause() }
    }

    @JavascriptInterface
    fun stop() {
        main.post { RekordCast.stop() }
    }

    @JavascriptInterface
    fun seek(seconds: Double) {
        main.post { RekordCast.seek(seconds) }
    }
}
