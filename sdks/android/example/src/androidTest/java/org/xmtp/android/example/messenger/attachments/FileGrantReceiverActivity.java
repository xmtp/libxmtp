package org.xmtp.android.example.messenger.attachments;

import android.app.Activity;
import android.os.Bundle;
import android.os.Process;
import android.os.ResultReceiver;
import android.util.Log;
import java.io.InputStream;

/** Uses framework classes so the separate test APK UID needs no target runtime. */
public final class FileGrantReceiverActivity extends Activity {
  @Override public void onCreate(Bundle state) {
    super.onCreate(state);
    boolean readable = false;
    try (InputStream input = getContentResolver().openInputStream(getIntent().getData())) {
      readable = input != null && input.read() >= 0;
    } catch (Exception error) {
      Log.i("XmtpFileGrant", "Activity read denied: " + error.getClass().getSimpleName());
    }
    ResultReceiver receiver = getIntent().getParcelableExtra("result");
    Bundle result = new Bundle();
    result.putInt("uid", Process.myUid());
    Log.i("XmtpFileGrant", "Activity result uid=" + Process.myUid() + " readable=" + readable);
    if (receiver != null) receiver.send(readable ? 1 : 0, result);
    finish();
  }
}
