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
import android.media.AudioManager
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
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

    /*
     * Audio focus is left to the WebView: Chromium requests it as soon as the
     * <audio> element starts and pauses/resumes the element itself on losses
     * (calls, other music apps). Requesting it here too made the two steal it
     * from each other, and the loss handler paused the track right after it
     * started. The legacy app ignored focus changes for the same reason.
     */

    /** Headphones unplugged / Bluetooth off: pause right away, before it plays through the speaker. */
    private val noisyReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context?, intent: Intent?) {
            if (intent?.action == AudioManager.ACTION_AUDIO_BECOMING_NOISY && isPlaying) {
                RekordMediaBridge.send("pause")
            }
        }
    }

    override fun onCreate() {
        super.onCreate()
        createChannel()
        audioManager = getSystemService(Context.AUDIO_SERVICE) as AudioManager
        session = MediaSessionCompat(this, "RE-KORD").apply {
            setCallback(object : MediaSessionCompat.Callback() {
                override fun onPlay() = RekordMediaBridge.send("play")
                override fun onPause() = RekordMediaBridge.send("pause")
                override fun onSkipToNext() = RekordMediaBridge.send("nexttrack")
                override fun onSkipToPrevious() = RekordMediaBridge.send("previoustrack")
                override fun onSeekTo(pos: Long) =
                    RekordMediaBridge.send("seekto", pos / 1000.0)

                override fun onStop() {
                    RekordMediaBridge.send("pause")
                    stop(this@RekordMediaService)
                }
            })
            isActive = true
        }
        running = this
        // onStartCommand does the first render.
        applyCastTarget(rerender = false)
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
            // The music ended in the meantime (queue cleared right before play).
            // A service started with startForegroundService must still go into the
            // foreground, otherwise Android kills it with an exception: enter and
            // leave immediately.
            enterForeground(buildNotification(idle()))
            stop(this)
            return START_NOT_STICKY
        }
        render(state)
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
        if (state.playing && castTarget == null) {
            registerNoisy()
        } else {
            unregisterNoisy()
        }
        val notification = buildNotification(state)
        if (state.playing) {
            if (inForeground) {
                NotificationManagerCompat.from(this).notify(NOTIFICATION_ID, notification)
            } else {
                enterForeground(notification)
            }
        } else {
            // While paused nothing is playing: leave the foreground
            // (Android 14 demands it) but keep the notification, so playback can
            // be resumed from it. DETACH keeps it up after leaving.
            if (inForeground) {
                ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_DETACH)
                inForeground = false
            }
            NotificationManagerCompat.from(this).notify(NOTIFICATION_ID, notification)
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
            Logger.warn("RekordMedia: receiver already removed: ${e.message}")
        }
    }

    private fun enterForeground(notification: Notification) {
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
            Logger.warn("RekordMedia: cover art not downloaded: ${e.message}")
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

        /** Used by MainActivity to decide whether to keep the WebView awake. */
        val isPlaying: Boolean
            get() = latest?.playing == true

        /**
         * Called by the page on every track or state change: the first
         * time it starts the service, then it updates it.
         */
        fun publish(context: Context, state: NowPlaying) {
            latest = state
            val service = running
            if (service != null) {
                service.render(state)
                return
            }
            // The service is only created while a track is playing: it is the only case in
            // which Android allows starting a foreground service, and the only one where
            // it is needed. A paused track without a service has nothing to keep alive.
            if (!state.playing) return
            ContextCompat.startForegroundService(
                context,
                Intent(context, RekordMediaService::class.java),
            )
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
