package app.rekord.client

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.os.PowerManager
import android.webkit.JavascriptInterface
import android.webkit.WebView
import org.json.JSONObject

/**
 * Brano in riproduzione, cosi' come lo racconta il lettore nella WebView.
 *
 * L'audio suona dentro la pagina: qui non si riproduce niente, si ripete al
 * sistema cosa sta suonando perche' possa disegnare la notifica e la schermata
 * di blocco.
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
                Logger.warn("RekordMedia: stato non leggibile: ${e.message}")
                null
            }
        }
    }
}

/**
 * I comandi della notifica tornano al lettore per la strada che il client ha
 * gia' pronta per i gusci nativi: un evento nel DOM, gli stessi nomi di azione
 * della Media Session (vedi `src/lib/mediaSession.ts`).
 *
 * A schermo spento la consegna non e' scontata: la WebView puo' essere sospesa,
 * la CPU puo' riaddormentarsi prima che lo script giri, la pagina puo' essere
 * ancora in caricamento. Come nella 5.0: un wake lock di pochi secondi, la
 * WebView risvegliata, e lo script che risponde `true` solo se il lettore c'e'
 * (`window.__rekordNativeMediaReady`, alzata da `nativeMedia.ts` al primo stato
 * inviato). Altrimenti si riprova qualche volta, poi si lascia perdere.
 */
object RekordMediaBridge {
    @Volatile
    var webView: WebView? = null

    private const val WAKE_MS = 5_000L
    private const val RETRY_MS = 250L
    private const val MAX_ATTEMPTS = 12

    private val main = Handler(Looper.getMainLooper())
    private var wakeLock: PowerManager.WakeLock? = null

    /** Un solo comando in volo: un «play» seguito da «pause» vale come «pause». */
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
                Logger.warn("RekordMedia: wake lock non disponibile: ${e.message}")
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
        // Con l'app in secondo piano e la musica in pausa la WebView e' sospesa:
        // un play dalla notifica parlerebbe a un lettore addormentato. Prima si
        // riaccende, poi le si dice cosa fare.
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
            Logger.warn("RekordMedia: comando ${pending?.first} non consegnato")
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
            Logger.warn("RekordMedia: wake lock non rilasciato: ${e.message}")
        }
    }
}

/**
 * Superficie esposta alla pagina come `window.RekordMediaNative`. La WebView
 * carica solo il nostro bundle locale, quindi non c'e' pagina di terzi che possa
 * chiamarla.
 */
class RekordMedia(private val activity: MainActivity) {
    @JavascriptInterface
    fun update(json: String) {
        val state = NowPlaying.fromJson(json) ?: return
        activity.runOnUiThread {
            // Il permesso si chiede al primo brano, non all'avvio: prima c'e'
            // qualcosa da mostrare, poi si chiede di poterlo mostrare.
            if (state.playing) activity.ensureNotificationPermission()
            RekordMediaService.publish(activity, state)
        }
    }

    @JavascriptInterface
    fun stop() {
        activity.runOnUiThread { RekordMediaService.stop(activity) }
    }
}
