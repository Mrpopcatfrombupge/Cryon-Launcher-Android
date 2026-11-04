package dev.cryon.client;

import android.app.Activity;
import android.app.AlertDialog;
import android.app.PendingIntent;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.pm.PackageInfo;
import android.content.pm.PackageInstaller;
import android.content.pm.PackageManager;
import android.net.Uri;
import android.os.Build;
import android.os.Handler;
import android.os.Looper;
import android.provider.Settings;
import android.util.Log;
import android.widget.Toast;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import org.json.JSONArray;
import org.json.JSONObject;

/** Self-update against this project's GitHub Releases. */
final class Updates {
    private static final String TAG = "CryonUpdates";
    private static final String LATEST = "https://api.github.com/repos/"
            + "Mrpopcatfrombupge/Cryon-Launcher-Android/releases/latest";
    private static final String INSTALL_DONE = "dev.cryon.client.UPDATE_DONE";

    private final Activity screen;
    private final Context app;
    private final Handler main = new Handler(Looper.getMainLooper());
    private AlertDialog progress;

    private Updates(Activity screen) {
        this.screen = screen;
        this.app = screen.getApplication();
    }

    static void check(Activity screen) {
        new Updates(screen).run();
    }

    private void run() {
        note("Checking for updates");
        new Thread(() -> {
            String message;
            try {
                JSONObject release = get(LATEST);
                String tag = release.getString("tag_name");
                int[] newer = parse(tag);
                int[] mine = parse(installed());
                if (compare(newer, mine) <= 0) {
                    message = "You have the latest version (" + mine[0] + "."
                            + mine[1] + "." + mine[2] + ").";
                } else {
                    JSONObject found = release;
                    say(() -> confirm(found, tag));
                    return;
                }
            } catch (Exception failed) {
                Log.w(TAG, "update check failed", failed);
                message = "Could not check for updates: " + failed.getMessage();
            }
            String shown = message;
            say(() -> done(shown));
        }, "cryon-updates").start();
    }

    /** Ask before spending the user's data on the download. */
    private void confirm(JSONObject release, String tag) {
        dismiss();
        new AlertDialog.Builder(screen)
                .setTitle("Update available")
                .setMessage("Version " + tag + " is available. Download and install it?")
                .setPositiveButton("Update", (d, w) -> fetch(release, tag))
                .setNegativeButton("Later", (d, w) -> d.dismiss())
                .show();
    }

    private void fetch(JSONObject release, String tag) {
        note("Downloading " + tag);
        new Thread(() -> {
            String error = null;
            try {
                install(download(apkAsset(release)));
            } catch (Exception failed) {
                Log.w(TAG, "update failed", failed);
                error = failed.getMessage();
            }
            if (error != null) {
                String shown = error;
                say(() -> done("Update failed: " + shown));
            }
        }, "cryon-updates").start();
    }

    // -- the pieces ----------------------------------------------------------

    /** The first .apk asset attached to the newest release. */
    private static String apkAsset(JSONObject release) throws Exception {
        JSONArray assets = release.getJSONArray("assets");
        for (int i = 0; i < assets.length(); i++) {
            String name = assets.getJSONObject(i).getString("name");
            if (name.endsWith(".apk")) {
                return assets.getJSONObject(i).getString("browser_download_url");
            }
        }
        throw new IllegalStateException("release has no APK");
    }

    private static JSONObject get(String url) throws Exception {
        HttpURLConnection connection = open(url);
        try (InputStream in = connection.getInputStream();
             ByteArrayOutputStream body = new ByteArrayOutputStream()) {
            byte[] buffer = new byte[8192];
            for (int read; (read = in.read(buffer)) > 0; ) {
                body.write(buffer, 0, read);
            }
            return new JSONObject(new String(body.toByteArray(), "UTF-8"));
        } finally {
            connection.disconnect();
        }
    }

    private static HttpURLConnection open(String url) throws Exception {
        HttpURLConnection connection = (HttpURLConnection) new URL(url).openConnection();
        connection.setConnectTimeout(15000);
        connection.setReadTimeout(30000);
        connection.setRequestProperty("User-Agent", "Cryon-Launcher");
        connection.connect();
        if (connection.getResponseCode() >= 400) {
            throw new IllegalStateException("HTTP " + connection.getResponseCode());
        }
        return connection;
    }

    /** Streams the APK to the app's own external files directory. */
    private File download(String url) throws Exception {
        File updates = app.getExternalFilesDir("updates");
        if (updates == null) throw new IllegalStateException("no storage");
        File apk = new File(updates, "update.apk");
        HttpURLConnection connection = open(url);
        try (InputStream in = connection.getInputStream();
             OutputStream out = new FileOutputStream(apk)) {
            byte[] buffer = new byte[65536];
            for (int read; (read = in.read(buffer)) > 0; ) out.write(buffer, 0, read);
        } finally {
            connection.disconnect();
        }
        return apk;
    }

    /** Commits the APK to PackageInstaller; the system takes it from there. */
    private void install(File apk) throws Exception {
        PackageManager manager = app.getPackageManager();
        if (!manager.canRequestPackageInstalls()) {
            say(() -> {
                dismiss();
                Toast.makeText(app, "Allow installs from this app, then check again",
                        Toast.LENGTH_LONG).show();
                app.startActivity(new Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                        Uri.parse("package:" + app.getPackageName()))
                        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            });
            return;
        }
        PackageInstaller.Session session = null;
        try {
            PackageInstaller installer = manager.getPackageInstaller();
            PackageInstaller.SessionParams params = new PackageInstaller.SessionParams(
                    PackageInstaller.SessionParams.MODE_FULL_INSTALL);
            int id = installer.createSession(params);
            session = installer.openSession(id);
            try (InputStream in = new FileInputStream(apk);
                 OutputStream out = session.openWrite("update", 0, apk.length())) {
                byte[] buffer = new byte[65536];
                for (int read; (read = in.read(buffer)) > 0; ) out.write(buffer, 0, read);
                session.fsync(out);
            }
            Intent done = new Intent(INSTALL_DONE).setPackage(app.getPackageName());
            PendingIntent signal = PendingIntent.getBroadcast(app, id, done,
                    Build.VERSION.SDK_INT >= 31 ? PendingIntent.FLAG_IMMUTABLE : 0);
            BroadcastReceiver receiver = new BroadcastReceiver() {
                @Override public void onReceive(Context c, Intent intent) {
                    int status = intent.getIntExtra(
                            PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE);
                    String text = status == PackageInstaller.STATUS_PENDING_USER_ACTION
                            ? "Confirm the install in the system prompt"
                            : status == PackageInstaller.STATUS_SUCCESS
                            ? "Update installed" : "Install failed (" + status + ")";
                    Log.i(TAG, text);
                    say(() -> Toast.makeText(app, text, Toast.LENGTH_LONG).show());
                    if (status == PackageInstaller.STATUS_PENDING_USER_ACTION) {
                        Intent confirm = intent.getParcelableExtra(Intent.EXTRA_INTENT);
                        if (confirm != null) {
                            confirm.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
                            app.startActivity(confirm);
                        }
                    }
                }
            };
            IntentFilter filter = new IntentFilter(INSTALL_DONE);
            if (Build.VERSION.SDK_INT >= 33) {
                app.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED);
            } else {
                app.registerReceiver(receiver, filter);
            }
            session.commit(signal.getIntentSender());
            say(() -> {
                dismiss();
                Toast.makeText(app, "Installing the update", Toast.LENGTH_SHORT).show();
            });
        } finally {
            if (session != null) session.close();
        }
    }

    // -- version comparison --------------------------------------------------

    /** This app's versionName, e.g. "0.1.0". */
    private String installed() throws Exception {
        PackageInfo info = app.getPackageManager().getPackageInfo(app.getPackageName(), 0);
        return info.versionName;
    }

    /** "v0.1.0-notes" and "0.1.0-dev" both become {0, 1, 0}. */
    private static int[] parse(String version) {
        String cleaned = version.replaceAll("^[vV]", "").split("[-+ ]")[0];
        String[] parts = cleaned.split("\\.");
        int[] numbers = new int[3];
        for (int i = 0; i < 3 && i < parts.length; i++) {
            try {
                numbers[i] = Integer.parseInt(parts[i].trim());
            } catch (NumberFormatException ignored) {
                numbers[i] = 0;
            }
        }
        return numbers;
    }

    private static int compare(int[] a, int[] b) {
        for (int i = 0; i < 3; i++) {
            if (a[i] != b[i]) return a[i] < b[i] ? -1 : 1;
        }
        return 0;
    }

    // -- screen helpers ------------------------------------------------------

    private void note(String text) {
        say(() -> {
            if (progress == null) {
                progress = new AlertDialog.Builder(screen)
                        .setMessage(text)
                        .setCancelable(false)
                        .show();
            } else {
                progress.setMessage(text);
            }
        });
    }

    private void dismiss() {
        if (progress != null) {
            progress.dismiss();
            progress = null;
        }
    }

    private void done(String text) {
        dismiss();
        new AlertDialog.Builder(screen).setMessage(text)
                .setPositiveButton("OK", (d, w) -> d.dismiss()).show();
    }

    private void say(Runnable onMain) {
        main.post(() -> {
            if (!screen.isFinishing() && !screen.isDestroyed()) onMain.run();
        });
    }
}
