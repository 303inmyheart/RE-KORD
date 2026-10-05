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
   * Il tasto Indietro lo gestiamo noi (vedi [installBackHandler]): quello di
   * WryActivity, arrivato in fondo alla cronologia, chiude l'activity — e con lei
   * la WebView in cui suona la musica.
   */
  override val handleBackNavigation: Boolean = false

  /**
   * Registrato alla costruzione dell'activity, come vuole ComponentActivity: cosi'
   * non serve inventare un codice di richiesta che potrebbe pestare i piedi ai
   * plugin Tauri. La risposta non cambia niente: senza permesso la musica suona
   * comunque, resta senza notifica.
   */
  private val askNotifications =
    registerForActivityResult(ActivityResultContracts.RequestPermission()) { }

  override fun onCreate(savedInstanceState: Bundle?) {
    applyOrientationPolicy()
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    RekordMediaBridge.attach(applicationContext)
    // Google Cast: senza Play Services resta spento e la pagina non mostra il pulsante.
    RekordCast.init(applicationContext)
    RekordCast.bindActivity(this)
  }

  /**
   * Telefono in verticale, come la 5.0 (l'interfaccia compatta e' pensata cosi');
   * tablet e TV ruotano liberamente. Il confine e' quello di Android per i layout
   * "sw600dp": sotto i 600 dp di lato corto e' un telefono.
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
    // I nomi combaciano con quelli cercati da `src/lib/nativeMedia.ts` e
    // `src/lib/platform/downloads.ts`.
    webView.addJavascriptInterface(RekordMedia(this), "RekordMediaNative")
    webView.addJavascriptInterface(RekordFiles(applicationContext), "RekordFilesNative")
    // `src/lib/cast/androidCast.ts`.
    webView.addJavascriptInterface(RekordCastJs(), "RekordCastNative")
    installBackHandler(webView)
  }

  /**
   * In trasmissione i tasti volume regolano il Chromecast, non il telefono (a
   * schermo spento ci pensa la MediaSession di [RekordMediaService]).
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
   * Indietro: prima si risale la cronologia della pagina (il client spinge una
   * voce per ogni vista e per ogni finestra aperta, cosi' Indietro chiude quella).
   * In fondo alla cronologia l'app non si chiude: va in secondo piano come con il
   * tasto Home, e la musica continua — il servizio media tiene vivo il processo.
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
   * WryActivity mette in pausa la WebView quando l'app va in secondo piano, e la
   * pausa della WebView spegne anche l'audio della pagina. Se c'e' un brano che
   * suona la si riaccende subito: il servizio in foreground tiene vivo il
   * processo, quindi la musica continua a schermo spento.
   */
  override fun onPause() {
    super.onPause()
    // In trasmissione la coda la manda avanti la pagina (fine brano sul receiver →
    // brano successivo): anche in pausa deve restare sveglia per sentirlo.
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
    // Qui si arriva solo quando l'activity muore davvero (rimossa dai recenti,
    // processo recuperato dal sistema): Indietro la manda in secondo piano senza
    // distruggerla. Con l'activity muore la WebView, e l'audio sta dentro la
    // WebView: la notifica non deve sopravvivere a un brano che non suona piu'.
    // Un cambio di configurazione non passa di qui (configChanges nel manifest).
    if (RekordMediaBridge.webView === webView) RekordMediaBridge.webView = null
    webView = null
    RekordCast.bindActivity(null)
    if (!isChangingConfigurations) RekordMediaService.stop(this)
    super.onDestroy()
  }
}
