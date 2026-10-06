package app.rekord.client

import android.Manifest
import android.content.pm.ActivityInfo
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.os.Build
import android.os.Bundle
import android.view.KeyEvent
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat

class MainActivity : TauriActivity() {
  private var webView: WebView? = null
  private var notificationAsked = false

  /**
   * We handle the Back button ourselves (see [installBackHandler]): WryActivity's
   * handler, once it reaches the end of the history, closes the activity — and with
   * it the WebView where the music is playing.
   */
  override val handleBackNavigation: Boolean = false

  /**
   * Registered when the activity is constructed, as ComponentActivity requires: this
   * way there is no need to invent a request code that could clash with the Tauri
   * plugins. The answer changes nothing: without the permission the music still
   * plays, just without a notification.
   */
  private val askNotifications =
    registerForActivityResult(ActivityResultContracts.RequestPermission()) { }

  override fun onCreate(savedInstanceState: Bundle?) {
    applyOrientationPolicy()
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    RekordMediaBridge.attach(applicationContext)
    // Google Cast: without Play Services it stays off and the page doesn't show the button.
    RekordCast.init(applicationContext)
    RekordCast.bindActivity(this)
  }

  /**
   * Phones in portrait, as in 5.0 (the compact UI is designed that way);
   * tablets and TVs rotate freely. The threshold is Android's own for "sw600dp"
   * layouts: below 600 dp on the short side it is a phone.
   */
  private fun applyOrientationPolicy() {
    val config = resources.configuration
    val isTv = (config.uiMode and Configuration.UI_MODE_TYPE_MASK) ==
      Configuration.UI_MODE_TYPE_TELEVISION ||
      packageManager.hasSystemFeature(PackageManager.FEATURE_LEANBACK)
    requestedOrientation = if (!isTv && config.smallestScreenWidthDp < 600) {
      ActivityInfo.SCREEN_ORIENTATION_PORTRAIT
    } else {
      ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
    }
  }

  override fun onWebViewCreate(webView: WebView) {
    super.onWebViewCreate(webView)
    this.webView = webView
    RekordMediaBridge.webView = webView
    // The names match those looked up by `src/lib/nativeMedia.ts` and
    // `src/lib/platform/downloads.ts`.
    webView.addJavascriptInterface(RekordMedia(this), "RekordMediaNative")
    webView.addJavascriptInterface(RekordFiles(applicationContext), "RekordFilesNative")
    // `src/lib/cast/androidCast.ts`.
    webView.addJavascriptInterface(RekordCastJs(), "RekordCastNative")
    installBackHandler(webView)
  }

  /**
   * While casting, the volume keys control the Chromecast, not the phone (with the
   * screen off, [RekordMediaService]'s MediaSession takes care of it).
   */
  override fun dispatchKeyEvent(event: KeyEvent): Boolean {
    if (RekordCast.isConnected) {
      val direction = when (event.keyCode) {
        KeyEvent.KEYCODE_VOLUME_UP -> 1
        KeyEvent.KEYCODE_VOLUME_DOWN -> -1
        else -> 0
      }
      if (direction != 0) {
        if (event.action == KeyEvent.ACTION_DOWN) RekordCast.adjustVolume(direction)
        return true
      }
    }
    return super.dispatchKeyEvent(event)
  }

  /**
   * Back: first walk back through the page history (the client pushes an entry
   * for every view and every open dialog, so Back closes it).
   * At the end of the history the app doesn't close: it goes to the background as
   * with the Home button, and the music keeps playing — the media service keeps the process alive.
   */
  private fun installBackHandler(view: WebView) {
    onBackPressedDispatcher.addCallback(
      this,
      object : OnBackPressedCallback(true) {
        override fun handleOnBackPressed() {
          if (view.canGoBack()) {
            view.goBack()
          } else {
            moveTaskToBack(true)
          }
        }
      },
    )
  }

  /**
   * WryActivity pauses the WebView when the app goes to the background, and pausing
   * the WebView also silences the page's audio. If a track is playing, it is
   * resumed immediately: the foreground service keeps the process alive, so the
   * music keeps playing with the screen off.
   */
  override fun onPause() {
    super.onPause()
    // While casting, the page advances the queue (track ends on the receiver →
    // next track): even when paused it must stay awake to notice.
    if (RekordMediaService.isPlaying || RekordCast.isConnected) webView?.onResume()
  }

  fun ensureNotificationPermission() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
    if (notificationAsked) return
    notificationAsked = true
    val granted = ContextCompat.checkSelfPermission(
      this,
      Manifest.permission.POST_NOTIFICATIONS,
    ) == PackageManager.PERMISSION_GRANTED
    if (!granted) askNotifications.launch(Manifest.permission.POST_NOTIFICATIONS)
  }

  override fun onDestroy() {
    // We only get here when the activity really dies (removed from recents,
    // process reclaimed by the system): Back sends it to the background without
    // destroying it. The WebView dies with the activity, and the audio lives inside
    // the WebView: the notification must not outlive a track that no longer plays.
    // A configuration change doesn't come through here (configChanges in the manifest).
    if (RekordMediaBridge.webView === webView) RekordMediaBridge.webView = null
    webView = null
    RekordCast.bindActivity(null)
    if (!isChangingConfigurations) RekordMediaService.stop(this)
    super.onDestroy()
  }
}
