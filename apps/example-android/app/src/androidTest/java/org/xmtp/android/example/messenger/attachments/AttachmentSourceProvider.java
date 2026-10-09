package org.xmtp.android.example.messenger.attachments;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.database.Cursor;
import android.database.MatrixCursor;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import android.provider.OpenableColumns;
import java.io.FileNotFoundException;
import java.io.IOException;
import java.io.OutputStream;

/** A separate test provider streams bytes and supplies an optional false size hint. */
public final class AttachmentSourceProvider extends ContentProvider {
  @Override public boolean onCreate() { return true; }
  @Override public String getType(Uri uri) { return "application/octet-stream"; }
  @Override public Cursor query(Uri uri, String[] projection, String selection, String[] args, String sort) {
    String[] columns = projection == null
        ? new String[] { OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE } : projection;
    Object[] values = new Object[columns.length];
    for (int i = 0; i < columns.length; i++) {
      String size = uri.getQueryParameter("length");
      values[i] = OpenableColumns.DISPLAY_NAME.equals(columns[i])
          ? "../../same.bin" : size == null ? null : Long.valueOf(size);
    }
    MatrixCursor cursor = new MatrixCursor(columns);
    cursor.addRow(values);
    return cursor;
  }
  @Override public ParcelFileDescriptor openFile(Uri uri, String mode) throws FileNotFoundException {
    int size = Integer.parseInt(uri.getQueryParameter("bytes"));
    if (!"r".equals(mode) || size < 0 || size > 104857601) throw new FileNotFoundException("Invalid test source");
    final ParcelFileDescriptor[] pipe;
    try { pipe = ParcelFileDescriptor.createPipe(); }
    catch (IOException error) { throw new FileNotFoundException(error.getMessage()); }
    new Thread(() -> {
      try (OutputStream output = new ParcelFileDescriptor.AutoCloseOutputStream(pipe[1])) {
        byte[] chunk = new byte[8192];
        for (int i = 0; i < chunk.length; i++) chunk[i] = (byte) (i % 251);
        int left = size;
        while (left > 0) {
          int count = Math.min(left, chunk.length);
          output.write(chunk, 0, count);
          left -= count;
        }
      } catch (IOException ignored) { /* The caller can close an oversized source. */ }
    }).start();
    return pipe[0];
  }
  @Override public Uri insert(Uri uri, ContentValues values) { throw new UnsupportedOperationException("Read only"); }
  @Override public int update(Uri uri, ContentValues values, String selection, String[] args) { return 0; }
  @Override public int delete(Uri uri, String selection, String[] args) { return 0; }
}
