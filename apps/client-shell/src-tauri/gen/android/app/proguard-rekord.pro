# The WebView bridge methods are called only from JavaScript: R8 sees no call
# sites and would strip them in release, leaving the notification without data.
-keepclasseswithmembers class * {
    @android.webkit.JavascriptInterface <methods>;
}

# Google Cast: Play Services instantiates the OptionsProvider by name, from the manifest.
-keep class app.rekord.client.RekordCastOptionsProvider { *; }
