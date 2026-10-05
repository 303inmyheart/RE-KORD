import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

// La chiave di firma non sta nel repo: si dichiara in gen/android/keystore.properties,
// che i .gitignore qui accanto tengono fuori. Se il file manca la release si firma
// con la chiave di debug dell'SDK: l'APK resta ottimizzato e si installa subito
// (come l'APK della 5.0), ma non e' pubblicabile sul Play Store e gli aggiornamenti
// firmati poi con la chiave vera richiedono di disinstallarlo.
// `scripts/android-build.sh` spiega come crearla.
val keystoreFile = rootProject.file("keystore.properties")
val keystore = Properties().apply {
    if (keystoreFile.exists()) {
        keystoreFile.inputStream().use { load(it) }
    }
}

android {
    compileSdk = 36
    namespace = "app.rekord.client"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "app.rekord.client"
        minSdk = 26
        targetSdk = 36
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    signingConfigs {
        if (keystoreFile.exists()) {
            create("release") {
                keyAlias = keystore.getProperty("keyAlias")
                keyPassword = keystore.getProperty("keyPassword")
                // Percorso relativo a gen/android, oppure assoluto.
                storeFile = rootProject.file(keystore.getProperty("storeFile"))
                storePassword = keystore.getProperty("storePassword")
            }
        }
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            // L'hub sta sulla rete di casa e si raggiunge scrivendo un indirizzo tipo
            // http://192.168.1.20:7420: su un IP privato non c'e certificato da avere, e
            // col valore di serie (false) l'APK di release non parla con nessun hub.
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            signingConfig = if (keystoreFile.exists()) {
                signingConfigs.getByName("release")
            } else {
                signingConfigs.getByName("debug")
            }
            isMinifyEnabled = true
            proguardFiles(
                *fileTree(".") { include("**/*.pro") }
                    .plus(getDefaultProguardFile("proguard-android-optimize.txt"))
                    .toList().toTypedArray()
            )
        }
    }
    kotlinOptions {
        jvmTarget = "1.8"
    }
    buildFeatures {
        buildConfig = true
    }
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    // MediaSessionCompat, MediaStyle e MediaButtonReceiver per la notifica media.
    implementation("androidx.media:media:1.7.0")
    // Google Cast nativo (RekordCast.kt): stessa versione della 5.0. Il selettore dei
    // dispositivi viene da androidx.mediarouter (lo tira gia' dentro il Cast SDK,
    // qui e' esplicito perche' RekordCast lo usa direttamente).
    implementation("com.google.android.gms:play-services-cast-framework:22.0.0")
    implementation("androidx.mediarouter:mediarouter:1.6.0")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = "tauri.build.gradle.kts")