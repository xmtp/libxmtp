package org.xmtp.reactnative;

import android.os.Handler;
import android.os.Looper;
import androidx.annotation.NonNull;
import androidx.lifecycle.Lifecycle;
import androidx.lifecycle.LifecycleEventObserver;
import androidx.lifecycle.ProcessLifecycleOwner;
import com.facebook.react.ReactApplication;
import com.facebook.react.ReactHost;
import com.facebook.react.bridge.JavaScriptContextHolder;
import com.facebook.react.bridge.ReactApplicationContext;
import com.facebook.react.module.annotations.ReactModule;
import com.facebook.react.turbomodule.core.interfaces.CallInvokerHolder;
import java.io.File;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.FutureTask;
import kotlin.Unit;
import kotlin.jvm.functions.Function0;

@ReactModule(name = XmtpReactNativeModule.NAME)
public final class XmtpReactNativeModule extends NativeXmtpReactNativeSpec {
  public static final String NAME = "XmtpReactNative";
  private long nativeOwner;
  private boolean invalidated;
  private final ReactHost reactHost;
  private final Function0<Unit> beforeDestroy = () -> {
    retireBeforeRuntimeDestruction();
    return Unit.INSTANCE;
  };
  private static boolean observing;
  private static final LifecycleEventObserver lifecycleObserver = (owner, event) -> {
    if (event == Lifecycle.Event.ON_START) nativeSetForeground(true);
    if (event == Lifecycle.Event.ON_STOP) nativeSetForeground(false);
  };

  static { System.loadLibrary("xmtp_react_native"); }

  public XmtpReactNativeModule(ReactApplicationContext context) {
    super(context);
    if (!(context.getApplicationContext() instanceof ReactApplication)) {
      throw new IllegalStateException("XMTP requires a ReactApplication with a ReactHost");
    }
    reactHost = ((ReactApplication) context.getApplicationContext()).getReactHost();
    if (reactHost == null) {
      throw new IllegalStateException("XMTP requires a ReactHost before-destroy hook");
    }
    reactHost.addBeforeDestroyListener(beforeDestroy);
  }

  private void retireBeforeRuntimeDestruction() {
    final long owner;
    synchronized (this) {
      invalidated = true;
      owner = nativeOwner;
      nativeOwner = 0;
    }
    // Installation can be waiting for its first main-thread lifecycle read.
    // No native owner exists then, and waiting on JS would deadlock that read.
    if (owner == 0) return;
    nativeStop(owner);
    FutureTask<Void> retire = new FutureTask<>(() -> {
      nativeInvalidate(owner);
      return null;
    });
    ReactApplicationContext context = getReactApplicationContext();
    if (context.isOnJSQueueThread()) retire.run();
    else if (!context.runOnJSQueueThread(retire)) {
      throw new IllegalStateException("XMTP runtime retirement queue is unavailable");
    }
    // ReactHost invokes this hook before quitting its JS queue. Do not allow
    // destruction to advance until all generation-owned JSI values are gone.
    boolean interrupted = false;
    try {
      for (;;) {
        try {
          retire.get();
          break;
        } catch (InterruptedException error) {
          interrupted = true;
        }
      }
    } catch (ExecutionException error) {
      throw new IllegalStateException("XMTP runtime retirement failed", error.getCause());
    } finally {
      if (interrupted) Thread.currentThread().interrupt();
    }
  }

  @Override
  @NonNull
  public String getName() { return NAME; }

  private static boolean observeLifecycle() {
    FutureTask<Boolean> seed = new FutureTask<>(() -> {
      Lifecycle lifecycle = ProcessLifecycleOwner.get().getLifecycle();
      if (!observing) {
        // Install observation before reading the actual process state. This
        // also covers a headless/background process without a resumed Activity.
        lifecycle.addObserver(lifecycleObserver);
        observing = true;
      }
      return nativeSeedLifecycle(lifecycle.getCurrentState().isAtLeast(Lifecycle.State.STARTED));
    });
    if (Looper.myLooper() == Looper.getMainLooper()) seed.run();
    else new Handler(Looper.getMainLooper()).post(seed);
    try { return seed.get(); }
    catch (InterruptedException error) {
      Thread.currentThread().interrupt();
      throw new IllegalStateException("XMTP lifecycle initialization interrupted", error);
    } catch (ExecutionException error) {
      throw new IllegalStateException("XMTP lifecycle initialization failed", error.getCause());
    }
  }

  @Override
  public boolean install() {
    synchronized (this) {
      if (invalidated) return false;
      if (nativeOwner != 0) return true;
    }
    // Do not hold the module monitor while waiting on the UI thread.
    if (!observeLifecycle()) return false;
    synchronized (this) {
      if (invalidated) return false;
      if (nativeOwner != 0) return true;
      ReactApplicationContext context = getReactApplicationContext();
      JavaScriptContextHolder runtime = context.getJavaScriptContextHolder();
      CallInvokerHolder invoker = context.getJSCallInvokerHolder();
      if (runtime == null || runtime.get() == 0 || invoker == null) return false;
      nativeOwner = nativeInstall(runtime.get(), invoker);
      return nativeOwner != 0;
    }
  }

  @Override
  public String defaultStorageDirectory() {
    return new File(getReactApplicationContext().getFilesDir(), "xmtp_db").getAbsolutePath();
  }

  @Override
  public void invalidate() {
    reactHost.removeBeforeDestroyListener(beforeDestroy);
    synchronized (this) {
      invalidated = true;
      if (nativeOwner != 0) {
        // This is reachable only if a host bypasses the required ReactHost
        // before-destroy contract. Stop native work, but never enter a dead JS
        // queue from TurboModule invalidation.
        nativeStop(nativeOwner);
        throw new IllegalStateException("XMTP ReactHost did not retire its runtime before invalidation");
      }
    }
    super.invalidate();
  }

  private static native long nativeInstall(long runtime, CallInvokerHolder invoker);
  private static native void nativeStop(long owner);
  private static native void nativeInvalidate(long owner);
  private static native boolean nativeSeedLifecycle(boolean foreground);
  private static native void nativeSetForeground(boolean foreground);
}
