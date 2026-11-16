package dev.dioxus.main
import android.os.Handler
import android.os.Looper
import android.util.Base64
import android.webkit.JavascriptInterface
import android.webkit.WebView
import org.json.JSONArray
import java.io.OutputStreamWriter
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.IBinder
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.media3.common.MediaItem
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.datasource.BaseDataSource
import androidx.media3.datasource.DataSource
import androidx.media3.datasource.DataSpec
import androidx.media3.datasource.DefaultHttpDataSource
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.dash.DashMediaSource
import androidx.media3.exoplayer.dash.manifest.DashManifestParser
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaStyleNotificationHelper
import java.io.ByteArrayInputStream

typealias BuildConfig = com.example.NanopipeApp.BuildConfig

class MainActivity : WryActivity()

/** Logs every byte range the player asks for, so a 403 can be tied to an offset. */
private class LoggingDataSource(private val upstream: DataSource) : BaseDataSource(true) {
    override fun open(dataSpec: DataSpec): Long {
        Log.i(
            "NanoPipeRange",
            "open host=${dataSpec.uri.host}:${dataSpec.uri.port} pos=${dataSpec.position} len=${dataSpec.length} flags=${dataSpec.flags}"
        )
        return upstream.open(dataSpec)
    }

    override fun read(buffer: ByteArray, offset: Int, length: Int): Int =
        upstream.read(buffer, offset, length)

    override fun getUri(): Uri? = upstream.uri

    override fun close() {
        upstream.close()
    }
}

private class LoggingFactory(private val inner: DataSource.Factory) : DataSource.Factory {
    override fun createDataSource(): DataSource = LoggingDataSource(inner.createDataSource())
}

/**
 * Foreground service that owns the single ExoPlayer instance for the app.
 *
 * YouTube's CDN only serves BOUNDED Range requests: an open-ended range (what a
 * progressive source asks for) answers 302 then 403. Rust therefore hands in a
 * DASH isoff-on-demand manifest built from rustypipe's init/index ranges.
 */
class PlaybackService : Service() {
    companion object {
        const val CHANNEL_ID = "nanopipe_playback"
        const val NOTIFICATION_ID = 1
        private const val TAG = "NanoPipePlayback"

        @Volatile
        private var player: ExoPlayer? = null

        @Volatile
        private var session: MediaSession? = null

        @JvmStatic
        fun play(
            context: Context,
            url: String,
            title: String,
            userAgent: String,
            dashManifest: String,
        ) {
            Log.i(TAG, "play title=$title ua=[$userAgent] dash=${dashManifest.length}B urlLen=${url.length}")
            val intent = Intent(context, PlaybackService::class.java)
            intent.putExtra("url", url)
            intent.putExtra("title", title)
            intent.putExtra("userAgent", userAgent)
            intent.putExtra("dashManifest", dashManifest)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                context.startForegroundService(intent)
            } else {
                context.startService(intent)
            }
        }

        @JvmStatic
        fun pause() {
            player?.pause()
        }

        @JvmStatic
        fun resume() {
            player?.play()
        }

        @JvmStatic
        fun stop() {
            player?.stop()
            session?.release()
            session = null
            player?.release()
            player = null
        }

        @JvmStatic
        fun isPlaying(): Boolean = player?.isPlaying == true
    }

    private fun httpFactory(userAgent: String): DefaultHttpDataSource.Factory =
        DefaultHttpDataSource.Factory()
            .setUserAgent(userAgent)
            .setAllowCrossProtocolRedirects(true)

    private fun ensurePlayer(userAgent: String): ExoPlayer {
        val existing = player
        if (existing != null) return existing
        val created = ExoPlayer.Builder(this)
            .setMediaSourceFactory(DefaultMediaSourceFactory(LoggingFactory(httpFactory(userAgent))))
            .build()
        created.addListener(object : Player.Listener {
            override fun onPlayerError(error: PlaybackException) {
                Log.e(TAG, "player error ${error.errorCodeName}: ${error.message}", error.cause)
            }

            override fun onPlaybackStateChanged(playbackState: Int) {
                Log.i(TAG, "state=$playbackState isPlaying=${created.isPlaying}")
            }
        })
        player = created
        session = MediaSession.Builder(this, created).build()
        return created
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        createChannel()
        startForeground(NOTIFICATION_ID, buildNotification(intent?.getStringExtra("title")))

        val url = intent?.getStringExtra("url")
        if (!url.isNullOrEmpty()) {
            val userAgent = intent.getStringExtra("userAgent").orEmpty()
            val dash = intent.getStringExtra("dashManifest").orEmpty()
            val exo = ensurePlayer(userAgent)
            val started = dash.isNotEmpty() && playDash(exo, userAgent, dash)
            if (!started) {
                Log.w(TAG, "falling back to progressive playback")
                exo.setMediaItem(MediaItem.fromUri(url))
                exo.prepare()
            }
            exo.play()
        }
        return START_STICKY
    }

    private fun playDash(engine: ExoPlayer, userAgent: String, manifestXml: String): Boolean =
        try {
            val manifest = DashManifestParser()
                .parse(Uri.EMPTY, ByteArrayInputStream(manifestXml.toByteArray()))
            val source = DashMediaSource
                .Factory(LoggingFactory(httpFactory(userAgent)))
                .createMediaSource(manifest)
            engine.setMediaSource(source)
            engine.prepare()
            Log.i(TAG, "dash ready: ${manifest.periodCount} period(s)")
            true
        } catch (e: Exception) {
            Log.e(TAG, "dash setup failed, will fall back", e)
            false
        }

    override fun onDestroy() {
        player?.release()
        player = null
        super.onDestroy()
    }

    private fun createChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "Playback",
                NotificationManager.IMPORTANCE_LOW
            )
            getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
        }
    }

    private fun buildNotification(title: String?): Notification {
        val built = NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle(title ?: "NanoPipe")
            .setSmallIcon(android.R.drawable.ic_media_play)
            .setOngoing(true)
        val active = session
        if (active != null) {
            built.setStyle(MediaStyleNotificationHelper.MediaStyle(active))
        }
        return built.build()
    }
}


// ---- PO TOKEN (ported from NewPipe's PoTokenWebView) ----
// Generates a YouTube PO token by running Google's BotGuard program in a WebView,
// exactly like NewPipe does (po_token.html + JavaScriptUtil + PoTokenWebView).
private const val POT_TAG = "NanoPipePoToken"
private const val POT_API_KEY = "AIzaSyDyT5W0Jh49F30Pqqtyfdf7pDLFKLJoAnw"
private const val POT_REQUEST_KEY = "O43z0dpjhgX20SCx4KAo"
private const val POT_USER_AGENT =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.3"

private const val POT_HTML = """<!DOCTYPE html>
<html lang="en"><head><title></title><script>
/**
 * Factory method to create and load a BotGuardClient instance.
 * @param options - Configuration options for the BotGuardClient.
 * @returns A promise that resolves to a loaded BotGuardClient instance.
 */
function loadBotGuard(challengeData) {
  this.vm = this[challengeData.globalName];
  this.program = challengeData.program;
  this.vmFunctions = {};
  this.syncSnapshotFunction = null;

  if (!this.vm)
    throw new Error('[BotGuardClient]: VM not found in the global object');

  if (!this.vm.a)
    throw new Error('[BotGuardClient]: Could not load program');

  const vmFunctionsCallback = function (
    asyncSnapshotFunction,
    shutdownFunction,
    passEventFunction,
    checkCameraFunction
  ) {
    this.vmFunctions = {
      asyncSnapshotFunction: asyncSnapshotFunction,
      shutdownFunction: shutdownFunction,
      passEventFunction: passEventFunction,
      checkCameraFunction: checkCameraFunction
    };
  };

  this.syncSnapshotFunction = this.vm.a(this.program, vmFunctionsCallback, true, this.userInteractionElement, function () {/** no-op */ }, [ [], [] ])[0]

  // an asynchronous function runs in the background and it will eventually call
  // `vmFunctionsCallback`, however we need to manually tell JavaScript to pass
  // control to the things running in the background by interrupting this async
  // function in any way, e.g. with a delay of 1ms. The loop is most probably not
  // needed but is there just because.
  return new Promise(function (resolve, reject) {
    i = 0
    refreshIntervalId = setInterval(function () {
      if (!!this.vmFunctions.asyncSnapshotFunction) {
        resolve(this)
        clearInterval(refreshIntervalId);
      }
      if (i >= 10000) {
        reject("asyncSnapshotFunction is null even after 10 seconds")
        clearInterval(refreshIntervalId);
      }
      i += 1;
    }, 1);
  })
}

/**
 * Takes a snapshot asynchronously.
 * @returns The snapshot result.
 * @example
 * ```ts
 * const result = await botguard.snapshot({
 *   contentBinding: {
 *     c: "a=6&a2=10&b=SZWDwKVIuixOp7Y4euGTgwckbJA&c=1729143849&d=1&t=7200&c1a=1&c6a=1&c6b=1&hh=HrMb5mRWTyxGJphDr0nW2Oxonh0_wl2BDqWuLHyeKLo",
 *     e: "ENGAGEMENT_TYPE_VIDEO_LIKE",
 *     encryptedVideoId: "P-vC09ZJcnM"
 *    }
 * });
 *
 * console.log(result);
 * ```
 */
function snapshot(args) {
  return new Promise(function (resolve, reject) {
    if (!this.vmFunctions.asyncSnapshotFunction)
      return reject(new Error('[BotGuardClient]: Async snapshot function not found'));

    this.vmFunctions.asyncSnapshotFunction(function (response) { resolve(response) }, [
      args.contentBinding,
      args.signedTimestamp,
      args.webPoSignalOutput,
      args.skipPrivacyBuffer
    ]);
  });
}

function runBotGuard(challengeData) {
  const interpreterJavascript = challengeData.interpreterJavascript.privateDoNotAccessOrElseSafeScriptWrappedValue;

  if (interpreterJavascript) {
    new Function(interpreterJavascript)();
  } else throw new Error('Could not load VM');

  const webPoSignalOutput = [];
  return loadBotGuard({
    globalName: challengeData.globalName,
    globalObj: this,
    program: challengeData.program
  }).then(function (botguard) {
    return botguard.snapshot({ webPoSignalOutput: webPoSignalOutput })
  }).then(function (botguardResponse) {
    return { webPoSignalOutput: webPoSignalOutput, botguardResponse: botguardResponse }
  })
}

function obtainPoToken(webPoSignalOutput, integrityToken, identifier) {
  const getMinter = webPoSignalOutput[0];

  if (!getMinter)
    throw new Error('PMD:Undefined');

  const mintCallback = getMinter(integrityToken);

  if (!(mintCallback instanceof Function))
    throw new Error('APF:Failed');

  const result = mintCallback(identifier);

  if (!result)
    throw new Error('YNJ:Undefined');

  if (!(result instanceof Uint8Array))
    throw new Error('ODM:Invalid');

  return result;
}
</script></head><body></body></html>
"""

class PoTokenClient(private val context: Context) {
    private val main = Handler(Looper.getMainLooper())
    private val ready = CountDownLatch(1)
    private val produced = CountDownLatch(1)

    @Volatile private var webView: WebView? = null
    @Volatile private var integrityToken: String? = null
    @Volatile private var token: String? = null
    @Volatile private var failure: String? = null

    fun generate(identifier: String, timeoutMs: Long = 25_000): String? {
        main.post { start() }
        if (!ready.await(timeoutMs, TimeUnit.MILLISECONDS)) {
            Log.e(POT_TAG, "botguard initialization timed out")
            cleanup()
            return null
        }
        if (failure != null) {
            Log.e(POT_TAG, "botguard initialization failed: ${'$'}failure")
            cleanup()
            return null
        }
        main.post { mint(identifier) }
        produced.await(timeoutMs, TimeUnit.MILLISECONDS)
        cleanup()
        return token
    }

    private fun start() {
        val web = WebView(context)
        web.settings.javaScriptEnabled = true
        web.settings.userAgentString = POT_USER_AGENT
        web.addJavascriptInterface(Bridge(), "PoTokenBridge")
        web.loadDataWithBaseURL(
            "https://www.youtube.com",
            POT_HTML.replaceFirst("</script>", "\nPoTokenBridge.downloadAndRunBotguard()</script>"),
            "text/html",
            "utf-8",
            null,
        )
        webView = web
    }

    private inner class Bridge {
        @JavascriptInterface
        fun downloadAndRunBotguard() {
            post(
                "https://www.youtube.com/api/jnn/v1/Create",
                "[ \"${'$'}POT_REQUEST_KEY\" ]",
            ) { body ->
                val challenge = parseChallengeData(body)
                main.post {
                    webView?.evaluateJavascript(
                        """try {
                            data = ${'$'}challenge
                            runBotGuard(data).then(function (result) {
                                this.webPoSignalOutput = result.webPoSignalOutput
                                PoTokenBridge.onRunBotguardResult(result.botguardResponse)
                            }, function (error) {
                                PoTokenBridge.onJsInitializationError(error + "\n" + error.stack)
                            })
                        } catch (error) {
                            PoTokenBridge.onJsInitializationError(error + "\n" + error.stack)
                        }""",
                        null,
                    )
                }
            }
        }

        @JavascriptInterface
        fun onJsInitializationError(error: String) {
            failure = error
            ready.countDown()
        }

        @JavascriptInterface
        fun onRunBotguardResult(botguardResponse: String) {
            post(
                "https://www.youtube.com/api/jnn/v1/GenerateIT",
                "[ \"${'$'}POT_REQUEST_KEY\", \"${'$'}botguardResponse\" ]",
            ) { body ->
                val parsed = parseIntegrityTokenData(body)
                integrityToken = parsed.first
                main.post {
                    webView?.evaluateJavascript("this.integrityToken = ${'$'}{parsed.first}", null)
                    ready.countDown()
                }
            }
        }

        @JavascriptInterface
        fun onToken(u8: String) {
            token = u8ToBase64(u8)
            produced.countDown()
        }

        @JavascriptInterface
        fun onMintError(error: String) {
            failure = error
            produced.countDown()
        }
    }

    private fun mint(identifier: String) {
        val web = webView
        val integrity = integrityToken
        if (web == null || integrity == null) {
            produced.countDown()
            return
        }
        val u8 = stringToU8(identifier)
        web.evaluateJavascript(
            """try {
                var u8Identifier = ${'$'}u8;
                var poTokenU8 = obtainPoToken(webPoSignalOutput, this.integrityToken, u8Identifier);
                var s = "";
                for (var i = 0; i < poTokenU8.length; i++) { if (i !== 0) s += ","; s += poTokenU8[i]; }
                PoTokenBridge.onToken(s);
            } catch (error) {
                PoTokenBridge.onMintError(String(error));
            }""",
            null,
        )
    }

    private fun post(url: String, data: String, onBody: (String) -> Unit) {
        Thread {
            try {
                val conn = URL(url).openConnection() as HttpURLConnection
                conn.requestMethod = "POST"
                conn.setRequestProperty("User-Agent", POT_USER_AGENT)
                conn.setRequestProperty("Accept", "application/json")
                conn.setRequestProperty("Content-Type", "application/json+protobuf")
                conn.setRequestProperty("x-goog-api-key", POT_API_KEY)
                conn.setRequestProperty("x-user-agent", "grpc-web-javascript/0.1")
                conn.doOutput = true
                OutputStreamWriter(conn.outputStream, "UTF-8").use { it.write(data) }
                val code = conn.responseCode
                val body = (if (code == 200) conn.inputStream else conn.errorStream)
                    ?.bufferedReader()?.use { it.readText() }.orEmpty()
                if (code == 200) {
                    onBody(body)
                } else {
                    failure = "HTTP ${'$'}code: ${'$'}body"
                    ready.countDown()
                }
            } catch (e: Exception) {
                failure = e.toString()
                ready.countDown()
            }
        }.start()
    }

    private fun cleanup() {
        main.post {
            webView?.let { web ->
                web.loadUrl("about:blank")
                web.destroy()
            }
            webView = null
        }
    }

    companion object {
        /** Called from Rust over JNI. */
        @JvmStatic
        fun obtain(context: Context, identifier: String): String? =
            PoTokenClient(context).generate(identifier)
    }
}

private fun parseChallengeData(rawChallengeData: String): String {
    val scrambled = JSONArray(rawChallengeData)
    val challengeData = if (scrambled.length() > 1 && !scrambled.isNull(1)) {
        JSONArray(descramble(scrambled.getString(1)))
    } else {
        scrambled.getJSONArray(0)
    }
    val interpreter = challengeData.optJSONArray(1)
    val resourceUrl = challengeData.optJSONArray(2)
    val out = org.json.JSONObject()
    out.put("messageId", challengeData.getString(0))
    val js = org.json.JSONObject()
    js.put(
        "privateDoNotAccessOrElseSafeScriptWrappedValue",
        findString(interpreter),
    )
    js.put(
        "privateDoNotAccessOrElseTrustedResourceUrlWrappedValue",
        findString(resourceUrl),
    )
    out.put("interpreterJavascript", js)
    out.put("interpreterHash", challengeData.getString(3))
    out.put("program", challengeData.getString(4))
    out.put("globalName", challengeData.getString(5))
    out.put("clientExperimentsStateBlob", challengeData.optString(7, ""))
    return out.toString()
}

private fun findString(array: JSONArray?): String {
    if (array == null) return ""
    for (i in 0 until array.length()) {
        if (!array.isNull(i) && array.opt(i) is String) return array.getString(i)
    }
    return ""
}

private fun parseIntegrityTokenData(raw: String): Pair<String, Long> {
    val array = JSONArray(raw)
    val token = newUint8Array(base64ToBytes(array.getString(0)))
    return token to array.optLong(1, 0L)
}

private fun descramble(scrambled: String): String {
    val bytes = base64ToBytes(scrambled)
    for (i in bytes.indices) bytes[i] = (bytes[i] + 97).toByte()
    return String(bytes, Charsets.UTF_8)
}

private fun base64ToBytes(base64: String): ByteArray {
    val normalized = base64.replace('-', '+').replace('_', '/').replace('.', '=')
    return Base64.decode(normalized, Base64.DEFAULT)
}

private fun newUint8Array(contents: ByteArray): String =
    "new Uint8Array([" + contents.joinToString(",") { (it.toInt() and 0xFF).toString() } + "])"

private fun stringToU8(identifier: String): String =
    newUint8Array(identifier.toByteArray(Charsets.UTF_8))

private fun u8ToBase64(poToken: String): String {
    val bytes = poToken.split(",").map { it.trim().toInt().toByte() }.toByteArray()
    return Base64.encodeToString(bytes, Base64.NO_WRAP or Base64.URL_SAFE or Base64.NO_PADDING)
}
