package app.rekord.client

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.ServiceInfo
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.media.AudioAttributes
import android.media.AudioManager
import android.media.AudioPlaybackConfiguration
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.PowerManager
import android.os.SystemClock
import android.support.v4.media.MediaMetadataCompat
import android.support.v4.media.session.MediaSessionCompat
import android.support.v4.media.session.PlaybackStateCompat
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import androidx.media.VolumeProviderCompat
import androidx.media.session.MediaButtonReceiver
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.Executors

/**
 * Keeps playback alive when the app goes to the background and draws the
 * system media notification.
 *
 * The sound stays in the WebView: this service has no player. It does two things
 * the page cannot do. First, it is a foreground service, so Android doesn't
 * freeze the process with the screen off. Second, it holds a MediaSession: the
 * notification, the lock screen controls and the headphone button come from it,
 * and then travel back to the page via [RekordMediaBridge].
 *
 * MediaSessionCompat is deprecated in favour of Media3, but Media3 requires
 * implementing a `Player`: here the player is an <audio> tag on the other side of
 * a JavaScript bridge, so a hand-made session remains the shortest path.
 *
 * Reliability with the screen off (see docs/ANDROID.md, "Screen off, network, car"):
 * - it stays in the foreground while the page plays or means to play (loading,
 *   retrying, reconnecting), and for [PAUSE_GRACE_MS] after a pause. Leaving the
 *   foreground at every pause made the next play re-enter it from the background,
 *   which Android 12+ refuses (ForegroundServiceStartNotAllowedException) after a
 *   call or another app's audio paused the track;
 * - while the page means to play it holds a partial wake lock and a Wi-Fi lock:
 *   between two tracks no audio comes out, nothing else keeps the CPU awake, and
 *   the radio's power save stalled the stream "after a while";
 * - a watchdog sends the page a heartbeat every [WATCHDOG_MS] (its own timers are
 *   throttled while hidden), so the player can notice a stalled stream and reload it;
 * - network changes (Wi-Fi to mobile data in the car) are passed to the page;
 * - after another app's audio ends, [InterruptionPolicy] decides whether to resume.
 */
@Suppress("DEPRECATION")
class RekordMediaService : Service() {
    private lateinit var session: MediaSessionCompat
    private val artLoader = Executors.newSingleThreadExecutor()
    private val main = Handler(Looper.getMainLooper())

    @Volatile
    private var artUrl: String? = null
    private var art: Bitmap? = null
    private var inForeground = false

    private lateinit var audioManager: AudioManager
    private var noisyRegistered = false

    private val policy = InterruptionPolicy()
    private var wakeLock: PowerManager.WakeLock? = null
    private var resumeWakeLock: PowerManager.WakeLock? = null
    private var wifiLock: WifiManager.WifiLock? = null
    private var locksHeld = false
    private var connectivity: ConnectivityManager? = null
    private var networkCallbackRegistered = false
    private var modeListener: Any? = null

    /** Last time (elapsedRealtime) the page played or meant to play. */
    private var lastActiveAt = 0L
    /** Since when the page means to play without sound coming out (0 = not). */
    private var silentIntentSince = 0L
    private var lastPositionMs = -1L
    private var lastProgressAt = 0L
    private var stallReported = false
    private var lastTransition = ""
    private var lastTitle = ""
    private var currentNetwork: Network? = null
    private var networkSeen = false

    /*
     * Audio focus is left to the WebView: Chromium requests it as soon as the
     * <audio> element starts and pauses/resumes the element itself on losses
     * (calls, other music apps). Requesting it here too made the two steal it
     * from each other, and the loss handler paused the track right after it
     * started. The legacy app ignored focus changes for the same reason.
     */

    /**
     * Headphones unplugged / Bluetooth off: pause right away, before it plays through
     * the speaker. Also registered while an interruption may still resume: a car that
     * disconnects during a call must not get the music back on the phone's speaker.
     */
    private val noisyReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context?, intent: Intent?) {
            if (intent?.action != AudioManager.ACTION_AUDIO_BECOMING_NOISY) return
            policy.onNoisy()?.let { RekordLog.i("interruption: $it") }
            if (isPlaying) {
                RekordMediaBridge.send("pause", source = "audio output disconnected")
            }
            latest?.let { render(it) }
        }
    }

    /** Heartbeat for the page while it means to play (see the class comment). */
    private val watchdog = object : Runnable {
        override fun run() {
            val state = latest ?: return
            if (!locksHeld) return
            val now = SystemClock.elapsedRealtime()
            // Re-armed on every beat: a timeout keeps a lost release from draining the battery.
            acquireWakeLock()
            val stalledMs = if (state.playing) now - lastProgressAt else 0L
            if (state.playing && stalledMs >= STALL_REPORT_MS && !stallReported) {
                stallReported = true
                RekordLog.w("watchdog: page says playing, no progress for ${stalledMs / 1000}s")
            }
            // Silent and someone else has the audio (a call, another app): the page
            // must not answer with play() / reload, that would fight for the focus.
            val otherAudio = !state.playing && (inCall() || otherAudioActive())
            RekordMediaBridge.signal(
                "rekord:playback-watchdog",
                "{stalledMs:$stalledMs,otherAudio:$otherAudio}",
            )
            applyLocks(state)
            if (locksHeld) main.postDelayed(this, WATCHDOG_MS)
        }
    }

    /** Re-evaluates the foreground state when the pause grace runs out. */
    private val graceCheck = Runnable { latest?.let { render(it) } }

    /** Re-evaluates a pending interruption (quiet window, call end, expiry). */
    private val interruptionCheck = Runnable { checkInterruption("timer") }

    /** Other apps' audio starting and stopping (no focus request of our own). */
    private val playbackCallback = object : AudioManager.AudioPlaybackCallback() {
        override fun onPlaybackConfigChanged(configs: MutableList<AudioPlaybackConfiguration>?) {
            if (policy.armed) checkInterruption("audio change")
        }
    }

    private val networkCallback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) {
            main.post { onNetwork(network, available = true) }
        }

        override fun onLost(network: Network) {
            main.post { onNetwork(network, available = false) }
        }
    }

    override fun onCreate() {
        super.onCreate()
        createChannel()
        audioManager = getSystemService(Context.AUDIO_SERVICE) as AudioManager
        session = MediaSessionCompat(this, "RE-KORD").apply {
            // Notification, lock screen, headset, Bluetooth / car controls.
            setCallback(object : MediaSessionCompat.Callback() {
                override fun onPlay() = RekordMediaBridge.send("play", source = "media session")
                override fun onPause() {
                    policy.onUserPause()?.let { RekordLog.i("interruption: $it") }
                    RekordMediaBridge.send("pause", source = "media session")
                }

                override fun onSkipToNext() =
                    RekordMediaBridge.send("nexttrack", source = "media session")

                override fun onSkipToPrevious() =
                    RekordMediaBridge.send("previoustrack", source = "media session")

                override fun onSeekTo(pos: Long) =
                    RekordMediaBridge.send("seekto", pos / 1000.0)

                override fun onStop() {
                    policy.onUserPause()
                    RekordMediaBridge.send("pause", source = "media session stop")
                    stop(this@RekordMediaService)
                }
            })
            isActive = true
        }
        running = this
        createLocks()
        watchSystem()
        // onStartCommand does the first render.
        applyCastTarget(rerender = false)
    }

    private fun createLocks() {
        val pm = getSystemService(Context.POWER_SERVICE) as? PowerManager
        wakeLock = pm?.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "Rekord:Playback")?.apply {
            setReferenceCounted(false)
        }
        resumeWakeLock = pm?.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "Rekord:Resume")?.apply {
            setReferenceCounted(false)
        }
        val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
        // HIGH_PERF is what keeps the radio out of power save with the screen off
        // (LOW_LATENCY only applies while the app is in front with the screen on).
        @Suppress("DEPRECATION")
        wifiLock = wifi?.createWifiLock(WifiManager.WIFI_MODE_FULL_HIGH_PERF, "Rekord:Playback")?.apply {
            setReferenceCounted(false)
        }
    }

    /** Other apps' audio, calls and the network: observed, never requested. */
    private fun watchSystem() {
        try {
            audioManager.registerAudioPlaybackCallback(playbackCallback, main)
        } catch (e: Exception) {
            RekordLog.w("audio playback callback unavailable: ${e.message}")
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            try {
                val listener = AudioManager.OnModeChangedListener {
                    if (policy.armed) checkInterruption("audio mode $it")
                }
                audioManager.addOnModeChangedListener(mainExecutor, listener)
                modeListener = listener
            } catch (e: Exception) {
                RekordLog.w("audio mode listener unavailable: ${e.message}")
            }
        }
        val cm = getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager
        connectivity = cm
        try {
            cm?.registerDefaultNetworkCallback(networkCallback, main)
            networkCallbackRegistered = cm != null
        } catch (e: Exception) {
            RekordLog.w("network callback unavailable: ${e.message}")
        }
    }

    private fun unwatchSystem() {
        try {
            audioManager.unregisterAudioPlaybackCallback(playbackCallback)
        } catch (e: Exception) {
            RekordLog.w("audio playback callback not removed: ${e.message}")
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            (modeListener as? AudioManager.OnModeChangedListener)?.let {
                try {
                    audioManager.removeOnModeChangedListener(it)
                } catch (e: Exception) {
                    RekordLog.w("audio mode listener not removed: ${e.message}")
                }
            }
            modeListener = null
        }
        if (networkCallbackRegistered) {
            networkCallbackRegistered = false
            try {
                connectivity?.unregisterNetworkCallback(networkCallback)
            } catch (e: Exception) {
                RekordLog.w("network callback not removed: ${e.message}")
            }
        }
    }

    private fun transportOf(network: Network): String {
        val caps = try {
            connectivity?.getNetworkCapabilities(network)
        } catch (e: Exception) {
            null
        } ?: return "unknown"
        return when {
            caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) -> "wifi"
            caps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) -> "cellular"
            caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) -> "ethernet"
            caps.hasTransport(NetworkCapabilities.TRANSPORT_BLUETOOTH) -> "bluetooth"
            caps.hasTransport(NetworkCapabilities.TRANSPORT_VPN) -> "vpn"
            else -> "other"
        }
    }

    /**
     * The default network changed. The stream's connection belonged to the old
     * one: the page checks its decks and reconnects at once instead of waiting for
     * its (throttled) stall timers.
     */
    private fun onNetwork(network: Network, available: Boolean) {
        if (available) {
            if (network == currentNetwork) return
            currentNetwork = network
            val transport = transportOf(network)
            // Registering the callback reports the current network: not a change.
            val first = !networkSeen
            networkSeen = true
            if (first) {
                RekordLog.i("network: $transport")
                return
            }
            RekordLog.i("network changed: now $transport")
            if (latest?.wantsPlay == true) {
                RekordMediaBridge.signal("rekord:network", "{available:true,transport:'$transport'}")
            }
        } else {
            if (network != currentNetwork) return
            currentNetwork = null
            RekordLog.i("network lost")
            if (latest?.wantsPlay == true) {
                RekordMediaBridge.signal("rekord:network", "{available:false}")
            }
        }
    }

    /** Is some other app (or a call) producing sound right now? */
    private fun otherAudioActive(): Boolean {
        // Only called while our page is paused: whatever plays is someone else.
        // Apps get the anonymized list of *active* players.
        val configs = try {
            audioManager.activePlaybackConfigurations
        } catch (e: Exception) {
            return false
        }
        return configs.any {
            when (it.audioAttributes.usage) {
                // Key clicks and accessibility hints are not an interruption.
                AudioAttributes.USAGE_ASSISTANCE_SONIFICATION,
                AudioAttributes.USAGE_ASSISTANCE_ACCESSIBILITY -> false
                else -> true
            }
        }
    }

    private fun inCall(): Boolean = audioManager.mode != AudioManager.MODE_NORMAL

    private fun checkInterruption(trigger: String) {
        main.removeCallbacks(interruptionCheck)
        val now = SystemClock.elapsedRealtime()
        val other = otherAudioActive()
        val call = inCall()
        when (val d = policy.decide(now, other, call)) {
            InterruptionPolicy.Decision.None -> Unit
            is InterruptionPolicy.Decision.Wait -> {
                val delay = d.recheckMs ?: ARMED_RECHECK_MS
                // A short wait (the quiet window) must not be stretched by deep sleep.
                if (d.recheckMs != null) holdResumeWake(delay + 1_000L)
                main.postDelayed(interruptionCheck, delay)
            }
            InterruptionPolicy.Decision.Resume -> {
                RekordLog.i("interruption over ($trigger): resuming")
                holdResumeWake(RESUME_WAKE_MS)
                RekordMediaBridge.send("play", source = "resume after interruption")
                main.postDelayed(interruptionCheck, RESUME_RECHECK_MS)
            }
            is InterruptionPolicy.Decision.Expired -> {
                RekordLog.i("interruption: not resuming, ${d.reason}")
                latest?.let { render(it) }
            }
        }
    }

    private fun holdResumeWake(ms: Long) {
        try {
            resumeWakeLock?.acquire(ms)
        } catch (e: Exception) {
            RekordLog.w("resume wake lock unavailable: ${e.message}")
        }
    }

    /** A new state from the page (as opposed to a redraw for the cover art). */
    private fun accept(state: NowPlaying) {
        val now = SystemClock.elapsedRealtime()
        if (state.title != lastTitle) {
            lastTitle = state.title
            lastPositionMs = -1L
            RekordLog.i("track: ${state.title.take(80)}")
        }
        if (!state.playing || state.positionMs != lastPositionMs) {
            lastPositionMs = state.positionMs
            lastProgressAt = now
            if (stallReported && state.playing) RekordLog.i("watchdog: progress again")
            stallReported = false
        }
        if (state.playing || state.wantsPlay) lastActiveAt = now
        silentIntentSince = when {
            state.playing || !state.wantsPlay -> 0L
            silentIntentSince == 0L -> now
            else -> silentIntentSince
        }
        val transition = when {
            state.playing -> "playing"
            state.wantsPlay -> "waiting to play (loading, retrying or reconnecting)"
            state.pauseReason.isNotEmpty() -> "paused (${state.pauseReason})"
            else -> "paused"
        }
        if (transition != lastTransition) {
            lastTransition = transition
            RekordLog.i("page: $transition")
        }
        policy.onPageState(state.playing, state.pauseReason, now)?.let {
            RekordLog.i("interruption: $it")
        }
        render(state)
        if (policy.armed) checkInterruption("page state")
    }

    /**
     * While casting, the music comes out of the Chromecast: no pause when
     * headphones are unplugged, and the volume keys with the screen off go
     * to the Cast device. The notification stays, showing the device name.
     */
    private fun applyCastTarget(rerender: Boolean = true) {
        if (castTarget != null) {
            unregisterNoisy()
            session.setPlaybackToRemote(castVolume())
        } else {
            session.setPlaybackToLocal(AudioManager.STREAM_MUSIC)
        }
        if (rerender) latest?.let { render(it) }
    }

    private fun castVolume(): VolumeProviderCompat {
        val steps = 20
        val current = (RekordCast.volume() * steps).toInt().coerceIn(0, steps)
        return object : VolumeProviderCompat(VOLUME_CONTROL_ABSOLUTE, steps, current) {
            override fun onSetVolumeTo(volume: Int) {
                val level = volume.coerceIn(0, steps)
                RekordCast.setVolume(level / steps.toDouble())
                currentVolume = level
            }

            override fun onAdjustVolume(direction: Int) {
                val level = (currentVolume + direction).coerceIn(0, steps)
                RekordCast.setVolume(level / steps.toDouble())
                currentVolume = level
            }
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        MediaButtonReceiver.handleIntent(session, intent)
        val state = latest
        if (state == null) {
            // The music ended in the meantime (queue cleared right before play), or a
            // media button woke us with no page to play (the car connecting after the
            // app was closed: nothing to resume, no autoplay).
            // A service started with startForegroundService must still go into the
            // foreground, otherwise Android kills it with an exception: enter and
            // leave immediately.
            enterForeground(buildNotification(idle()))
            stop(this)
            return START_NOT_STICKY
        }
        // Started with startForegroundService: startForeground is owed in any case.
        if (!inForeground) enterForeground(buildNotification(state))
        accept(state)
        return START_NOT_STICKY
    }

    /** Rewrites the session and notification with the state just received from the page. */
    private fun render(state: NowPlaying) {
        requestArt(state.artworkUrl)
        session.setMetadata(
            MediaMetadataCompat.Builder()
                .putString(MediaMetadataCompat.METADATA_KEY_TITLE, state.title)
                .putString(MediaMetadataCompat.METADATA_KEY_ARTIST, state.artist)
                .putString(MediaMetadataCompat.METADATA_KEY_ALBUM, state.album)
                .putLong(MediaMetadataCompat.METADATA_KEY_DURATION, state.durationMs)
                .apply { art?.let { putBitmap(MediaMetadataCompat.METADATA_KEY_ALBUM_ART, it) } }
                .build(),
        )
        session.setPlaybackState(
            PlaybackStateCompat.Builder()
                .setState(
                    if (state.playing) PlaybackStateCompat.STATE_PLAYING
                    else PlaybackStateCompat.STATE_PAUSED,
                    state.positionMs,
                    if (state.playing) 1f else 0f,
                )
                .setActions(
                    PlaybackStateCompat.ACTION_PLAY or
                        PlaybackStateCompat.ACTION_PAUSE or
                        PlaybackStateCompat.ACTION_PLAY_PAUSE or
                        PlaybackStateCompat.ACTION_SKIP_TO_NEXT or
                        PlaybackStateCompat.ACTION_SKIP_TO_PREVIOUS or
                        PlaybackStateCompat.ACTION_SEEK_TO or
                        PlaybackStateCompat.ACTION_STOP,
                )
                .build(),
        )
        if ((state.playing || policy.armed) && castTarget == null) {
            registerNoisy()
        } else {
            unregisterNoisy()
        }
        applyLocks(state)
        val notification = buildNotification(state)
        main.removeCallbacks(graceCheck)
        val now = SystemClock.elapsedRealtime()
        val graceLeft = PAUSE_GRACE_MS - (now - lastActiveAt)
        if (state.playing || state.wantsPlay || policy.armed || graceLeft > 0) {
            if (inForeground) {
                NotificationManagerCompat.from(this).notify(NOTIFICATION_ID, notification)
            } else {
                enterForeground(notification)
            }
            if (!state.playing && !state.wantsPlay && !policy.armed) {
                main.postDelayed(graceCheck, graceLeft + 1_000L)
            }
        } else {
            // Paused for a while: leave the foreground but keep the notification,
            // so playback can be resumed from it and it can be swiped away.
            // DETACH keeps it up after leaving. Until now the service stayed in the
            // foreground: a pause from a call or from another app ends with a play
            // that comes from the background, and from the background Android 12+
            // no longer lets a service back into the foreground.
            if (inForeground) {
                ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_DETACH)
                inForeground = false
                RekordLog.i("service: out of the foreground (paused for ${PAUSE_GRACE_MS / 60_000} min)")
            }
            NotificationManagerCompat.from(this).notify(NOTIFICATION_ID, notification)
        }
    }

    /**
     * CPU and Wi-Fi stay awake while the page plays or means to play. A page that
     * keeps failing to start (hub gone for good) lets go after [SILENT_INTENT_MAX_MS].
     */
    private fun applyLocks(state: NowPlaying) {
        val now = SystemClock.elapsedRealtime()
        val silentTooLong = silentIntentSince != 0L && now - silentIntentSince > SILENT_INTENT_MAX_MS
        val want = state.playing || (state.wantsPlay && !silentTooLong)
        if (want == locksHeld) return
        locksHeld = want
        if (want) {
            acquireWakeLock()
            try {
                wifiLock?.acquire()
            } catch (e: Exception) {
                RekordLog.w("wifi lock unavailable: ${e.message}")
            }
            main.removeCallbacks(watchdog)
            main.postDelayed(watchdog, WATCHDOG_MS)
            RekordLog.i("locks: CPU and Wi-Fi held")
        } else {
            releaseLocks()
            RekordLog.i(
                if (silentTooLong) "locks: released, nothing has played for ${SILENT_INTENT_MAX_MS / 60_000} min"
                else "locks: released",
            )
        }
    }

    private fun acquireWakeLock() {
        try {
            wakeLock?.acquire(WAKE_LOCK_TIMEOUT_MS)
        } catch (e: Exception) {
            RekordLog.w("wake lock unavailable: ${e.message}")
        }
    }

    private fun releaseLocks() {
        main.removeCallbacks(watchdog)
        try {
            if (wakeLock?.isHeld == true) wakeLock?.release()
            if (wifiLock?.isHeld == true) wifiLock?.release()
        } catch (e: Exception) {
            RekordLog.w("locks not released: ${e.message}")
        }
    }

    private fun registerNoisy() {
        if (noisyRegistered) return
        ContextCompat.registerReceiver(
            this,
            noisyReceiver,
            IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY),
            ContextCompat.RECEIVER_NOT_EXPORTED,
        )
        noisyRegistered = true
    }

    private fun unregisterNoisy() {
        if (!noisyRegistered) return
        noisyRegistered = false
        try {
            unregisterReceiver(noisyReceiver)
        } catch (e: IllegalArgumentException) {
            RekordLog.w("receiver already removed: ${e.message}")
        }
    }

    private fun enterForeground(notification: Notification) {
        try {
            ServiceCompat.startForeground(
                this,
                NOTIFICATION_ID,
                notification,
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                    ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK
                } else {
                    0
                },
            )
            inForeground = true
            RekordLog.i("service: in the foreground")
        } catch (e: Exception) {
            // Android 12+ refuses the foreground from the background
            // (ForegroundServiceStartNotAllowedException, an IllegalStateException).
            // Uncaught it closed the app; now the music goes on without the
            // protection, and the notification is still drawn.
            inForeground = false
            RekordLog.w("service: foreground refused: ${e.javaClass.simpleName}: ${e.message}")
            NotificationManagerCompat.from(this).notify(NOTIFICATION_ID, notification)
        }
    }

    private fun idle() = NowPlaying(
        title = getString(R.string.app_name),
        artist = "",
        album = "",
        artworkUrl = null,
        playing = false,
        durationMs = 0L,
        positionMs = 0L,
    )

    private fun buildNotification(state: NowPlaying): Notification {
        val open = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val stop = MediaButtonReceiver.buildMediaButtonPendingIntent(
            this,
            PlaybackStateCompat.ACTION_STOP,
        )
        val subtitle = listOf(state.artist, state.album)
            .filter { it.isNotEmpty() }
            .joinToString(" · ")
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_rekord_note)
            .setContentTitle(state.title)
            .setContentText(subtitle)
            .setSubText(castTarget?.let { getString(R.string.cast_playing_on, it) })
            .setLargeIcon(art)
            .setContentIntent(open)
            .setDeleteIntent(stop)
            .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
            .setOngoing(state.playing)
            .setShowWhen(false)
            .setOnlyAlertOnce(true)
            .addAction(
                android.R.drawable.ic_media_previous,
                getString(R.string.media_previous),
                MediaButtonReceiver.buildMediaButtonPendingIntent(
                    this,
                    PlaybackStateCompat.ACTION_SKIP_TO_PREVIOUS,
                ),
            )
            .addAction(
                if (state.playing) android.R.drawable.ic_media_pause
                else android.R.drawable.ic_media_play,
                getString(if (state.playing) R.string.media_pause else R.string.media_play),
                MediaButtonReceiver.buildMediaButtonPendingIntent(
                    this,
                    PlaybackStateCompat.ACTION_PLAY_PAUSE,
                ),
            )
            .addAction(
                android.R.drawable.ic_media_next,
                getString(R.string.media_next),
                MediaButtonReceiver.buildMediaButtonPendingIntent(
                    this,
                    PlaybackStateCompat.ACTION_SKIP_TO_NEXT,
                ),
            )
            .setStyle(
                androidx.media.app.NotificationCompat.MediaStyle()
                    .setMediaSession(session.sessionToken)
                    .setShowActionsInCompactView(0, 1, 2)
                    .setShowCancelButton(true)
                    .setCancelButtonIntent(stop),
            )
            .build()
    }

    /**
     * The cover art comes from the hub over HTTP: it is downloaded once per URL and
     * redrawn when ready, without blocking the notification.
     */
    private fun requestArt(url: String?) {
        if (url == null) {
            artUrl = null
            art = null
            return
        }
        if (url == artUrl) return
        artUrl = url
        art = null
        artLoader.execute {
            val bitmap = downloadArt(url)
            // If the track changed in the meantime, this cover art is no longer needed.
            if (bitmap == null || url != artUrl) return@execute
            main.post {
                if (url != artUrl) return@post
                art = bitmap
                latest?.let { render(it) }
            }
        }
    }

    private fun downloadArt(url: String): Bitmap? {
        var connection: HttpURLConnection? = null
        return try {
            connection = (URL(url).openConnection() as HttpURLConnection).apply {
                connectTimeout = 5_000
                readTimeout = 5_000
                instanceFollowRedirects = true
            }
            if (connection.responseCode !in 200..299) return null
            connection.inputStream.use { BitmapFactory.decodeStream(it) }
        } catch (e: Exception) {
            RekordLog.w("cover art not downloaded: ${e.message}")
            null
        } finally {
            connection?.disconnect()
        }
    }

    private fun createChannel() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val channel = NotificationChannel(
            CHANNEL_ID,
            getString(R.string.media_channel_name),
            NotificationManager.IMPORTANCE_LOW,
        ).apply {
            description = getString(R.string.media_channel_description)
            setShowBadge(false)
            lockscreenVisibility = Notification.VISIBILITY_PUBLIC
        }
        getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }

    /** If the app is closed from recents the WebView dies, and the audio with it. */
    override fun onTaskRemoved(rootIntent: Intent?) {
        super.onTaskRemoved(rootIntent)
        stop(this)
    }

    override fun onDestroy() {
        running = null
        RekordLog.i("service: stopped")
        releaseLocks()
        try {
            if (resumeWakeLock?.isHeld == true) resumeWakeLock?.release()
        } catch (e: Exception) {
            RekordLog.w("resume wake lock not released: ${e.message}")
        }
        unwatchSystem()
        // `latest` is not cleared here: if the service dies without going through stop()
        // (memory, system) the page is still playing, and the next state it sends
        // will restart it. Whoever stops the music takes care of clearing it.
        artLoader.shutdownNow()
        main.removeCallbacksAndMessages(null)
        unregisterNoisy()
        session.isActive = false
        session.release()
        NotificationManagerCompat.from(this).cancel(NOTIFICATION_ID)
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    companion object {
        private const val CHANNEL_ID = "rekord.playback"
        private const val NOTIFICATION_ID = 1

        /** In the foreground this long after a pause (Media3's default is the same). */
        private const val PAUSE_GRACE_MS = 10 * 60_000L
        /** Page heartbeat while it means to play. */
        private const val WATCHDOG_MS = 10_000L
        /** "Playing" without a position update for this long goes in the log. */
        private const val STALL_REPORT_MS = 25_000L
        /** Wanting to play with nothing coming out: locks are dropped after this. */
        private const val SILENT_INTENT_MAX_MS = 10 * 60_000L
        /** Safety timeout of the playback wake lock, re-armed by every heartbeat. */
        private const val WAKE_LOCK_TIMEOUT_MS = 10 * 60_000L
        /** Interruption armed, waiting on a call or other audio: poll as a backstop. */
        private const val ARMED_RECHECK_MS = 30_000L
        private const val RESUME_WAKE_MS = 15_000L
        private const val RESUME_RECHECK_MS = 10_500L

        @Volatile
        private var running: RekordMediaService? = null

        @Volatile
        private var latest: NowPlaying? = null

        /** Name of the connected Cast device, null when the music plays here. */
        @Volatile
        private var castTarget: String? = null

        /**
         * Called by [RekordCast] when a Cast session starts or ends. The
         * service is not started from here: it is started by the first "playing"
         * state that the page mirrors from the receiver.
         */
        fun setCastTarget(deviceName: String?) {
            if (castTarget == deviceName) return
            castTarget = deviceName
            running?.applyCastTarget()
        }

        val isPlaying: Boolean
            get() = latest?.playing == true

        /**
         * Used by MainActivity to decide whether to keep the WebView running when it
         * goes to the background: while the page plays or means to, and while the
         * service is still in the foreground after a pause (a pause caused by
         * another app may be resumed from here).
         */
        val keepWebViewAwake: Boolean
            get() {
                val state = latest ?: return false
                return state.playing || state.wantsPlay || running?.inForeground == true
            }

        /**
         * Called by the page on every track or state change: the first
         * time it starts the service, then it updates it.
         */
        fun publish(context: Context, state: NowPlaying) {
            latest = state
            val service = running
            if (service != null) {
                service.accept(state)
                return
            }
            // The service is only created while a track is playing (or about to): it is
            // the only case in which Android allows starting a foreground service (the
            // user just pressed play, in front of the app), and the only one where it is
            // needed. A paused track without a service has nothing to keep alive.
            if (!state.wantsPlay) return
            try {
                ContextCompat.startForegroundService(
                    context,
                    Intent(context, RekordMediaService::class.java),
                )
            } catch (e: Exception) {
                // From the background on Android 12+ (the system had stopped the
                // service): the music plays on, without notification for now.
                RekordLog.w("service not started: ${e.javaClass.simpleName}: ${e.message}")
            }
        }

        fun stop(context: Context) {
            latest = null
            val service = running
            if (service != null) {
                ServiceCompat.stopForeground(service, ServiceCompat.STOP_FOREGROUND_REMOVE)
                service.inForeground = false
                service.stopSelf()
            } else {
                context.stopService(Intent(context, RekordMediaService::class.java))
            }
        }
    }
}
