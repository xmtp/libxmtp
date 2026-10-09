package org.xmtp.android.example.messenger.attachments;

import android.app.Activity;
import android.content.BroadcastReceiver;
import android.content.ClipData;
import android.content.Context;
import android.content.Intent;
import android.net.Uri;
import android.os.Bundle;
import android.widget.TextView;
import java.lang.ref.WeakReference;

/** Holds an actual Activity Result until the test returns its selected URI. */
public final class AttachmentPickerActivity extends Activity {
    public static final String COMPLETE = "org.xmtp.android.example.test.COMPLETE_ATTACHMENT_PICKER";
    private static WeakReference<AttachmentPickerActivity> current = new WeakReference<>(null);

    public static final class ResultReceiver extends BroadcastReceiver {
        @Override public void onReceive(Context context, Intent request) {
            AttachmentPickerActivity activity = current.get();
            if (activity != null) activity.complete(request.getData());
        }
    }

    private void complete(Uri uri) {
        if (uri == null) {
            setResult(Activity.RESULT_CANCELED);
        } else {
            Intent response = new Intent().setData(uri);
            response.setClipData(ClipData.newRawUri("Selected file", uri));
            response.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION);
            setResult(Activity.RESULT_OK, response);
        }
        finish();
    }

    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        current = new WeakReference<>(this);
        TextView label = new TextView(this);
        label.setText("Attachment picker ready");
        setContentView(label);
    }

    @Override public void onDestroy() {
        if (current.get() == this) current.clear();
        super.onDestroy();
    }
}
