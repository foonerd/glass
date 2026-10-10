package io.github.foonerd.glass;

import android.Manifest;
import android.content.ActivityNotFoundException;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.net.Uri;
import android.net.wifi.WifiManager;
import android.os.Build;
import android.os.Bundle;
import android.os.Environment;
import android.provider.Settings;
import android.view.WindowManager;

import org.libsdl.app.SDLActivity;

/**
 * The display's activity: SDL's, with the display run as a remote, the
 * screen kept on, a multicast lock so the Wi-Fi delivers the broadcasts
 * players announce themselves with, and, when the display asks, the
 * system's screen that grants the app access to the device's files.
 */
public class GlassActivity extends SDLActivity {
    /**
     * The display's message when a theme from a folder on this device
     * cannot be read: Android lets an app read what other apps put under
     * Download only with access to the device's files. COMMAND_USER + 1 on
     * the native side.
     */
    private static final int ASK_STORAGE_ACCESS = COMMAND_USER + 1;

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

    @Override
    protected boolean onUnhandledMessage(int command, Object param) {
        if (command != ASK_STORAGE_ACCESS) {
            return super.onUnhandledMessage(command, param);
        }
        askStorageAccess();
        return true;
    }

    /**
     * The system's screen where the app is given access to the device's
     * files: "All files access" from Android 11, the read permission's
     * own question before it. Nothing when the access is there already.
     */
    private void askStorageAccess() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            if (Environment.isExternalStorageManager()) {
                return;
            }
            try {
                startActivity(new Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION, Uri.parse("package:" + getPackageName())));
            } catch (ActivityNotFoundException e) {
                startActivity(new Intent(Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION));
            }
        } else if (checkSelfPermission(Manifest.permission.READ_EXTERNAL_STORAGE) != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(new String[] { Manifest.permission.READ_EXTERNAL_STORAGE }, 1);
        }
    }
}
