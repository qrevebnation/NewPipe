package dev.dioxus.main

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.IBinder
import androidx.media3.common.MediaItem
import androidx.media3.exoplayer.ExoPlayer

typealias BuildConfig = com.example.NanopipeApp.BuildConfig

class MainActivity : WryActivity()

/**
 * Foreground service that owns the single ExoPlayer instance for the app.
 * Managed from Rust over JNI via [play] / [pause] / [stop].
 */
class PlaybackService : Service() {
    companion object {
        const val CHANNEL_ID = "nanopipe_playback"
        const val NOTIFICATION_ID = 1

        @Volatile
        private var player: ExoPlayer? = null

        @JvmStatic
        fun play(context: Context, url: String, title: String) {
            val intent = Intent(context, PlaybackService::class.java)
            intent.putExtra("url", url)
            intent.putExtra("title", title)
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
            player?.release()
            player = null
        }

        @JvmStatic
        fun isPlaying(): Boolean = player?.isPlaying == true
    }

    private fun ensurePlayer(): ExoPlayer {
        val existing = player
        if (existing != null) return existing
        val created = ExoPlayer.Builder(this).build()
        player = created
        return created
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        createChannel()
        val title = intent?.getStringExtra("title")
        startForeground(NOTIFICATION_ID, buildNotification(title))

        val url = intent?.getStringExtra("url")
        if (!url.isNullOrEmpty()) {
            val exo = ensurePlayer()
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
        val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Notification.Builder(this, CHANNEL_ID)
        } else {
            @Suppress("DEPRECATION")
            Notification.Builder(this)
        }
        return builder
            .setContentTitle(title ?: "NanoPipe")
            .setSmallIcon(android.R.drawable.ic_media_play)
            .setOngoing(true)
            .build()
    }
}
