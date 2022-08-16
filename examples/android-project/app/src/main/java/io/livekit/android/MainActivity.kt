package io.livekit.android

import android.os.Bundle
import androidx.appcompat.app.AppCompatActivity
import com.sun.jna.Library
import com.sun.jna.Native
import org.webrtc.PeerConnectionFactory

class MainActivity : AppCompatActivity() {

    /*interface LKLib : Library {
        companion object {
            internal val Instance: LKLib by lazy {
                Native.load("livekit_native", LKLib::class.java)
            }
        }

        fun test_rust()
    }*/

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        println("Started activity")

        val opt = PeerConnectionFactory.InitializationOptions.builder(applicationContext).setNativeLibraryName("livekit_native").createInitializationOptions()

        PeerConnectionFactory.initialize(opt)
        val factory = PeerConnectionFactory.builder().createPeerConnectionFactory()

        //LKLib.Instance.test_rust()

        println("Called Rust fnc")
        println("Now, start investigating the JNI platform specific stuff !")
    }
}
