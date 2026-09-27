package io.github.foonerd.glass;

import android.content.Context;
import android.net.wifi.WifiManager;
import android.os.Bundle;
import android.view.WindowManager;

import org.libsdl.app.SDLActivity;

/**
 * The display's activity: SDL's, with the display run as a remote, the
 * screen kept on, and a multicast lock so the Wi-Fi delivers the
 * broadcasts players announce themselves with.
 */
public class GlassActivity extends SDLActivity {
    private WifiManager.MulticastLock multicast;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        WifiManager wifi = (WifiManager) getApplicationContext().getSystemService(Context.WIFI_SERVICE);
        if (wifi != null) {
            multicast = wifi.createMulticastLock("glass");
            multicast.setReferenceCounted(false);
            multicast.acquire();
        }
    }

    @Override
    protected void onDestroy() {
        if (multicast != null && multicast.isHeld()) {
            multicast.release();
        }
        super.onDestroy();
    }

    @Override
    protected String[] getLibraries() {
        return new String[] { "SDL2", "main" };
    }

    @Override
    protected String[] getArguments() {
        return new String[] { "--remote" };
    }
}
