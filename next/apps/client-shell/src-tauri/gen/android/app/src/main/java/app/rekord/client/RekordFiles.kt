package app.rekord.client

import android.content.ContentValues
import android.content.Context
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.MediaStore
import android.util.Base64
import android.webkit.JavascriptInterface
import java.io.File
import java.io.FileOutputStream
import java.io.OutputStream
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicInteger

/**
 * Salvataggio dei file che la pagina costruisce in memoria (backup dell'hub,
 * profilo, pacchetto tema): nella WebView di Android `<a download href="blob:">`
 * non fa niente, e il DownloadManager non sa leggere un URL `blob:`.
 *
 * La pagina (`src/lib/platform/downloads.ts`) manda i byte a pezzi in base64:
 * `begin(nome, mime)` → id, `append(id, pezzo)` ripetuto, `finish(id)` → dove e'
 * finito il file. Da Android 10 si scrive in Download tramite MediaStore, senza
 * permessi; prima, nella cartella Download privata dell'app (visibile dal file
 * manager sotto Android/data/app.rekord.client).
 */
class RekordFiles(private val context: Context) {
    private class Pending(val stream: OutputStream, val uri: Uri?, val label: String)

    private val open = ConcurrentHashMap<Int, Pending>()
    private val ids = AtomicInteger(1)

    @JavascriptInterface
    fun begin(name: String, mime: String): Int {
        val safe = sanitize(name)
        val type = mime.ifBlank { "application/octet-stream" }
        return try {
            val pending = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                val values = ContentValues().apply {
                    put(MediaStore.Downloads.DISPLAY_NAME, safe)
                    put(MediaStore.Downloads.MIME_TYPE, type)
                    put(MediaStore.Downloads.IS_PENDING, 1)
                }
                val resolver = context.contentResolver
                val uri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
                    ?: return -1
                val stream = resolver.openOutputStream(uri) ?: return -1
                Pending(stream, uri, "${Environment.DIRECTORY_DOWNLOADS}/$safe")
            } else {
                val dir = context.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS)
                    ?: File(context.filesDir, "downloads")
                dir.mkdirs()
                val file = uniqueFile(dir, safe)
                Pending(FileOutputStream(file), null, file.absolutePath)
            }
            val id = ids.getAndIncrement()
            open[id] = pending
            id
        } catch (e: Exception) {
            Logger.warn("RekordFiles: impossibile creare $safe: ${e.message}")
            -1
        }
    }

    @JavascriptInterface
    fun append(id: Int, base64: String): Boolean {
        val pending = open[id] ?: return false
        return try {
            pending.stream.write(Base64.decode(base64, Base64.DEFAULT))
            true
        } catch (e: Exception) {
            Logger.warn("RekordFiles: scrittura fallita: ${e.message}")
            abort(id)
            false
        }
    }

    /** Torna il percorso leggibile, o una stringa vuota se qualcosa e' andato storto. */
    @JavascriptInterface
    fun finish(id: Int): String {
        val pending = open.remove(id) ?: return ""
        return try {
            pending.stream.close()
            if (pending.uri != null && Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                val values = ContentValues().apply { put(MediaStore.Downloads.IS_PENDING, 0) }
                context.contentResolver.update(pending.uri, values, null, null)
            }
            pending.label
        } catch (e: Exception) {
            Logger.warn("RekordFiles: chiusura fallita: ${e.message}")
            ""
        }
    }

    @JavascriptInterface
    fun abort(id: Int) {
        val pending = open.remove(id) ?: return
        try {
            pending.stream.close()
        } catch (e: Exception) {
            Logger.warn("RekordFiles: ${e.message}")
        }
        pending.uri?.let {
            try {
                context.contentResolver.delete(it, null, null)
            } catch (e: Exception) {
                Logger.warn("RekordFiles: ${e.message}")
            }
        }
    }

    private fun sanitize(name: String): String {
        val base = name.substringAfterLast('/').substringAfterLast('\\')
        val cleaned = base.map { c ->
            if (c.isISOControl() || c in "<>:\"|?*") '_' else c
        }.joinToString("").trim().trim('.')
        return cleaned.ifEmpty { "rekord-download" }
    }

    private fun uniqueFile(dir: File, name: String): File {
        var file = File(dir, name)
        if (!file.exists()) return file
        val dot = name.lastIndexOf('.')
        val stem = if (dot > 0) name.substring(0, dot) else name
        val ext = if (dot > 0) name.substring(dot) else ""
        var n = 1
        while (file.exists()) {
            file = File(dir, "$stem ($n)$ext")
            n++
        }
        return file
    }
}
