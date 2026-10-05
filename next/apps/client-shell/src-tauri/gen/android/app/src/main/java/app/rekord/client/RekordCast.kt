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
 * Opzioni Cast (letta da Play Services tramite il meta-data nel manifest):
 * Default Media Receiver, come nella 5.0 e come il sender web.
 *
 * Notifica e MediaSession del Cast SDK sono spente: la notifica la disegna gia'
 * [RekordMediaService] con lo stato che la pagina rispecchia dal receiver, e due
 * notifiche per lo stesso brano sarebbero un doppione.
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
 * Google Cast nativo (porting di `RekordCastManager` della 5.0).
 *
 * La 5.0 lasciava suonare la pagina in muto e ricaricava sul receiver ogni stato
 * della Media Session. Qui decide tutto la pagina: `src/lib/cast/androidCast.ts`
 * e' un `CastBackend` come il sender web, chiama i metodi di [RekordCastJs] e
 * riceve lo stato dal receiver come evento DOM `rekord:cast` — la stessa strada
 * di [RekordMediaBridge] per i comandi della notifica.
 *
 * Dettagli dell'evento (`detail`):
 *  - `{type:"status", status:{...}}`: vedi [statusJson], ~1 Hz durante la riproduzione;
 *  - `{type:"session", event:"cancelled"|"startFailed"|"unavailable", code?}`;
 *  - `{type:"result", id, ok, error?}`: esito di un `load`.
 *
 * Tutto il Cast SDK vive sul thread principale: i metodi del ponte arrivano dal
 * thread di JavaBridge e si limitano a postare.
 */
object RekordCast {
    private const val EVENT = "rekord:cast"
    /** Passo dei tasti volume sul dispositivo Cast (0..1). */
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
    /** Sessione in avvio o in ripresa: il selettore chiuso non vuol dire «annullato». */
    private var starting = false

    private var activity: WeakReference<Activity>? = null
    private var chooser: Dialog? = null

    private var lastSent = ""

    /** Ultimo stato, letto in modo sincrono dalla pagina con `getStatus()`. */
    @Volatile
    var latestStatus: String = "{\"ready\":false,\"supported\":false}"
        private set

    /** Serve a MainActivity (WebView sveglia, tasti volume) e alla notifica. */
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
     * Da chiamare sul thread principale (MainActivity.onCreate). Senza Play
     * Services, o con una versione che il Cast SDK non accetta, Cast resta
     * semplicemente non disponibile: il pulsante nella pagina non compare.
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
                    Logger.warn("RekordCast: CastContext non disponibile: ${e.message}")
                    finishInit(null, "cast-context")
                }
        } catch (e: Exception) {
            executor.shutdown()
            Logger.warn("RekordCast: Cast SDK non disponibile: ${e.message}")
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
            // Sessione ripresa all'avvio (app riaperta mentre il Chromecast suonava).
            ctx.sessionManager.currentCastSession?.takeIf { it.isConnected }?.let { attach(it) }
        } catch (e: Exception) {
            Logger.warn("RekordCast: inizializzazione fallita: ${e.message}")
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

    // ── Stato verso la pagina ─────────────────────────────────────────────

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

    /** Stato grezzo: la traduzione in `CastStatus` la fa `androidCastStatus.ts`. */
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
                    /* Sessione in chiusura: il volume non si legge piu'. */
                }
            }
        } catch (e: Exception) {
            Logger.warn("RekordCast: stato non leggibile: ${e.message}")
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
        // JSONObject produce un letterale JS valido: si incolla cosi' com'e'.
        val script = "window.dispatchEvent(new CustomEvent('$EVENT',{detail:$detail}))"
        main.post {
            try {
                view.evaluateJavascript(script, null)
            } catch (e: Exception) {
                Logger.warn("RekordCast: evento non consegnato: ${e.message}")
            }
        }
    }

    private fun result(id: Long, ok: Boolean, error: String? = null) {
        val o = JSONObject().put("type", "result").put("id", id).put("ok", ok)
        if (error != null) o.put("error", error)
        dispatch(o)
    }

    // ── Comandi dalla pagina (sempre sul thread principale) ───────────────

    fun bindActivity(a: Activity?) {
        activity = a?.let { WeakReference(it) }
        if (a == null) {
            chooser?.dismiss()
            chooser = null
        }
    }

    /**
     * Selettore dei dispositivi di androidx.mediarouter. Scegliere un dispositivo
     * seleziona la rotta, e il Cast SDK avvia la sessione da solo; chiudere il
     * selettore senza scegliere si racconta alla pagina come «cancelled».
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
                // La scelta di una rotta chiude il selettore e la sessione parte un
                // attimo dopo: si guarda poco piu' in la' se e' partita davvero.
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
            Logger.warn("RekordCast: selettore non disponibile: ${e.message}")
            chooser = null
            dispatch(JSONObject().put("type", "session").put("event", "unavailable"))
        }
    }

    fun endSession(stopReceiver: Boolean) {
        try {
            castContext?.sessionManager?.endCurrentSession(stopReceiver)
        } catch (e: Exception) {
            Logger.warn("RekordCast: chiusura sessione: ${e.message}")
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

    /** Volume del dispositivo (0..1), per i tasti volume e la notifica. */
    fun volume(): Double = try {
        session?.volume ?: 0.0
    } catch (e: Exception) {
        0.0
    }

    fun setVolume(level: Double) {
        try {
            session?.volume = level.coerceIn(0.0, 1.0)
        } catch (e: Exception) {
            Logger.warn("RekordCast: volume non impostato: ${e.message}")
        }
    }

    fun adjustVolume(direction: Int) {
        if (direction == 0) return
        setVolume(volume() + direction * VOLUME_STEP)
    }
}

/**
 * Superficie esposta alla pagina come `window.RekordCastNative` (nome cercato da
 * `src/lib/cast/androidCast.ts`). Come per [RekordMedia], la WebView carica solo
 * il nostro bundle.
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
