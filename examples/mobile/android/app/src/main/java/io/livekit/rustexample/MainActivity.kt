package io.livekit.rustexample

import android.Manifest
import android.content.pm.PackageManager
import android.os.Bundle
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import io.livekit.rustexample.ui.theme.RustexampleTheme

class MainActivity : ComponentActivity() {

    companion object {
        private const val TAG = "MainActivity"

        // Default connection values - change these for your setup
        private const val DEFAULT_URL = "ws://"
        private const val DEFAULT_TOKEN = ""
    }

    private var app: App? = null
    private var mediaManager: MediaManager? = null

    private val isConnected = mutableStateOf(false)
    private val isAudioActive = mutableStateOf(false)
    private val statusMessage = mutableStateOf("Ready to connect")

    private val requestPermissionLauncher = registerForActivityResult(
        ActivityResultContracts.RequestPermission()
    ) { isGranted ->
        if (isGranted) {
            Log.i(TAG, "RECORD_AUDIO permission granted")
            startAudioAfterPermission()
        } else {
            Log.w(TAG, "RECORD_AUDIO permission denied")
            statusMessage.value = "Microphone permission denied"
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        // Initialize the LiveKit app
        app = App()

        // Check if native library is available
        if (app?.isNativeAvailable() != true) {
            statusMessage.value = "Native library not available"
        }

        setContent {
            RustexampleTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.background
                ) {
                    MainScreen(
                        isConnected = isConnected.value,
                        isAudioActive = isAudioActive.value,
                        statusMessage = statusMessage.value,
                        defaultUrl = DEFAULT_URL,
                        defaultToken = DEFAULT_TOKEN,
                        onConnectClick = { url, token -> connectToRoom(url, token) },
                        onDisconnectClick = { disconnectFromRoom() },
                        onStartAudioClick = { startAudio() },
                        onStopAudioClick = { stopAudio() }
                    )
                }
            }
        }
    }

    private fun connectToRoom(url: String, token: String) {
        val currentApp = app ?: run {
            statusMessage.value = "App not initialized"
            return
        }

        if (!currentApp.isNativeAvailable()) {
            statusMessage.value = "Native library not available"
            Log.e(TAG, "Native library not available")
            return
        }

        if (url.isBlank() || token.isBlank()) {
            statusMessage.value = "Server URL and token are required"
            return
        }

        // Initialize MediaManager with the App reference
        if (mediaManager == null) {
            mediaManager = MediaManager(this, currentApp)
        }

        statusMessage.value = "Connecting..."
        currentApp.connect(url, token)
    }
        // Update state after a short delay to allow connection to establish
        // In a real app, you'd want callbacks from the native side
        android.os.Handler(mainLooper).postDelayed({
            val connected = currentApp.isConnected()
            isConnected.value = connected
            statusMessage.value = if (connected) "Connected to room" else "Connection failed"
            Log.i(TAG, "Connection result: $connected")
        }, 1000)
    }

    private fun disconnectFromRoom() {
        stopAudio()
        app?.disconnect()
        isConnected.value = false
        statusMessage.value = "Disconnected"
        Log.i(TAG, "Disconnected from room")
    }

    private fun startAudio() {
        if (ContextCompat.checkSelfPermission(
                this,
                Manifest.permission.RECORD_AUDIO
            ) == PackageManager.PERMISSION_GRANTED
        ) {
            startAudioAfterPermission()
        } else {
            requestPermissionLauncher.launch(Manifest.permission.RECORD_AUDIO)
        }
    }

    private fun startAudioAfterPermission() {
        val success = mediaManager?.startAll() ?: false
        if (success) {
            isAudioActive.value = true
            statusMessage.value = "Audio active - mic and speaker running"
            Log.i(TAG, "Audio started")
        } else {
            statusMessage.value = "Failed to start audio"
            Log.e(TAG, "Failed to start audio")
        }
    }

    private fun stopAudio() {
        mediaManager?.stopAll()
        isAudioActive.value = false
        if (isConnected.value) {
            statusMessage.value = "Connected (audio stopped)"
        }
        Log.i(TAG, "Audio stopped")
    }

    override fun onDestroy() {
        super.onDestroy()
        mediaManager?.release()
        mediaManager = null
        app?.disconnect()
    }
}

@Composable
fun MainScreen(
    isConnected: Boolean,
    isAudioActive: Boolean,
    statusMessage: String,
    defaultUrl: String,
    defaultToken: String,
    onConnectClick: (String, String) -> Unit,
    onDisconnectClick: () -> Unit,
    onStartAudioClick: () -> Unit,
    onStopAudioClick: () -> Unit
) {
    val url = remember { mutableStateOf(defaultUrl) }
    val token = remember { mutableStateOf(defaultToken) }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(16.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Top
    ) {
        Text(
            text = "LiveKit Rust SDK Demo",
            style = MaterialTheme.typography.headlineMedium,
            modifier = Modifier.padding(bottom = 8.dp)
        )

        Text(
            text = statusMessage,
            style = MaterialTheme.typography.bodyMedium,
            color = if (isConnected) MaterialTheme.colorScheme.primary
                    else MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(bottom = 24.dp)
        )

        if (!isConnected) {
            OutlinedTextField(
                value = url.value,
                onValueChange = { url.value = it },
                label = { Text("Server URL") },
                modifier = Modifier.fillMaxWidth(),
                singleLine = true
            )

            Spacer(modifier = Modifier.height(8.dp))

            OutlinedTextField(
                value = token.value,
                onValueChange = { token.value = it },
                label = { Text("Access Token") },
                modifier = Modifier.fillMaxWidth(),
                singleLine = true,
                visualTransformation = PasswordVisualTransformation()
            )

            Spacer(modifier = Modifier.height(16.dp))

            Button(
                onClick = { onConnectClick(url.value, token.value) },
                modifier = Modifier.fillMaxWidth()
            ) {
                Text("Connect")
            }
        } else {
            // Connected state
            Column(
                modifier = Modifier.fillMaxWidth(),
                horizontalAlignment = Alignment.CenterHorizontally
            ) {
                if (!isAudioActive) {
                    Button(
                        onClick = onStartAudioClick,
                        modifier = Modifier.fillMaxWidth()
                    ) {
                        Text("Start Audio")
                    }

                    Text(
                        text = "Start audio to enable microphone capture and speaker playback",
                        style = MaterialTheme.typography.bodySmall,
                        modifier = Modifier.padding(top = 8.dp)
                    )
                } else {
                    Button(
                        onClick = onStopAudioClick,
                        modifier = Modifier.fillMaxWidth()
                    ) {
                        Text("Stop Audio")
                    }

                    Text(
                        text = "Microphone: Capturing and sending to LiveKit\nSpeaker: Playing remote participants",
                        style = MaterialTheme.typography.bodySmall,
                        modifier = Modifier.padding(top = 8.dp)
                    )
                }

                Spacer(modifier = Modifier.height(24.dp))

                Button(
                    onClick = onDisconnectClick,
                    modifier = Modifier.fillMaxWidth()
                ) {
                    Text("Disconnect")
                }
            }
        }

        Spacer(modifier = Modifier.weight(1f))

        Text(
            text = "Audio: 48kHz, Mono, 16-bit PCM\n10ms frames (480 samples)",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )
    }
}

@Preview(showBackground = true)
@Composable
fun MainScreenDisconnectedPreview() {
    RustexampleTheme {
        MainScreen(
            isConnected = false,
            isAudioActive = false,
            statusMessage = "Ready to connect",
            defaultUrl = "ws://localhost:7880",
            defaultToken = "your-token-here",
            onConnectClick = { _, _ -> },
            onDisconnectClick = {},
            onStartAudioClick = {},
            onStopAudioClick = {}
        )
    }
}

@Preview(showBackground = true)
@Composable
fun MainScreenConnectedPreview() {
    RustexampleTheme {
        MainScreen(
            isConnected = true,
            isAudioActive = true,
            statusMessage = "Audio active - mic and speaker running",
            defaultUrl = "ws://localhost:7880",
            defaultToken = "your-token-here",
            onConnectClick = { _, _ -> },
            onDisconnectClick = {},
            onStartAudioClick = {},
            onStopAudioClick = {}
        )
    }
}
