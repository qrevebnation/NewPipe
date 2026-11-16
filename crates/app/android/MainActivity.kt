package dev.dioxus.main

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
            "open pos=${dataSpec.position} len=${dataSpec.length} flags=${dataSpec.flags}"
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
