package org.xmtp.android.example.messenger.attachments;

import android.app.Service;
import android.content.Intent;
import android.net.Uri;
import android.os.Bundle;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.os.Message;
import android.os.Messenger;
import android.os.Process;
import android.os.RemoteException;
import android.util.Log;
import java.io.InputStream;

/** Reports read permission from the test APK UID over a framework Binder. */
public final class FileGrantReceiverService extends Service {
  private final Messenger receiver = new Messenger(new Handler(Looper.getMainLooper()) {
    @Override public void handleMessage(Message request) {
      boolean readable = false;
      Uri uri = Uri.parse(request.getData().getString("uri"));
      try (InputStream input = getContentResolver().openInputStream(uri)) {
        readable = input != null && input.read() >= 0;
      } catch (Exception error) {
        Log.i("XmtpFileGrant", "Service read denied: " + error.getClass().getSimpleName());
      }
      Message response = Message.obtain();
      response.arg1 = readable ? 1 : 0;
      Bundle result = new Bundle();
      result.putInt("uid", Process.myUid());
      response.setData(result);
      try { request.replyTo.send(response); }
      catch (RemoteException error) { Log.i("XmtpFileGrant", "Result receiver ended"); }
    }
  });
  @Override public IBinder onBind(Intent intent) { return receiver.getBinder(); }
}
