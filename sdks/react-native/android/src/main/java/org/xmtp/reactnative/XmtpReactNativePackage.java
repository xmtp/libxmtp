package org.xmtp.reactnative;

import androidx.annotation.NonNull;
import androidx.annotation.Nullable;
import com.facebook.react.BaseReactPackage;
import com.facebook.react.bridge.NativeModule;
import com.facebook.react.bridge.ReactApplicationContext;
import com.facebook.react.module.model.ReactModuleInfo;
import com.facebook.react.module.model.ReactModuleInfoProvider;
import java.util.Collections;

public final class XmtpReactNativePackage extends BaseReactPackage {
  @Override
  @Nullable
  public NativeModule getModule(@NonNull String name, @NonNull ReactApplicationContext context) {
    return XmtpReactNativeModule.NAME.equals(name) ? new XmtpReactNativeModule(context) : null;
  }

  @Override
  public ReactModuleInfoProvider getReactModuleInfoProvider() {
    return () -> Collections.singletonMap(
        XmtpReactNativeModule.NAME,
        new ReactModuleInfo(XmtpReactNativeModule.NAME, XmtpReactNativeModule.NAME,
            false, false, false, true));
  }
}
