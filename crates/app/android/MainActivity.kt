package dev.dioxus.main

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.IBinder
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.media3.common.MediaItem
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.datasource.DefaultHttpDataSource
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaStyleNotificationHelper

typealias BuildConfig = com.example.NanopipeApp.BuildConfig

class MainActivity : WryActivity()

/**
 * Foreground service that owns the single ExoPlayer instance for the app.
 * Managed from Rust over JNI via [play] / [pause] / [stop].
 *
 * YouTube's stream URLs are issued for a specific client, so the User-Agent that
 * Rust passes in is applied to every request.
 *
 * NOTE: googlevideo answers open-ended Range requests (what ExoPlayer uses for
 * progressive media) with 302 -> 403; it only serves bounded ranges. Playing
 * YouTube audio therefore needs a DASH source built from rustypipe's
 * init_range/index_range, which is still to be wired up.
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
        fun play(context: Context, url: String, title: String, userAgent: String) {
            Log.i(TAG, "play title=$title ua=[$userAgent] urlLen=${url.length}")
            val intent = Intent(context, PlaybackService::class.java)
            intent.putExtra("url", url)
            intent.putExtra("title", title)
            intent.putExtra("userAgent", userAgent)
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

    private fun ensurePlayer(userAgent: String): ExoPlayer {
        val existing = player
        if (existing != null) return existing
        val http = DefaultHttpDataSource.Factory()
            .setUserAgent(userAgent)
            .setAllowCrossProtocolRedirects(true)
        val created = ExoPlayer.Builder(this)
            .setMediaSourceFactory(DefaultMediaSourceFactory(http))
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
            val exo = ensurePlayer(intent?.getStringExtra("userAgent").orEmpty())
            exo.setMediaItem(MediaItem.fromUri(url))
            exo.prepare()
            exo.play()
        }
        return START_STICKY
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
