#include "../cpp/XmtpNative.h"
#include "../cpp/XmtpRuntime.h"
#include <ReactCommon/CallInvokerHolder.h>
#include <fbjni/fbjni.h>
#include <jni.h>

using xmtp::reactnative::Runtime;
using Owner = std::shared_ptr<Runtime>;

extern "C" JNIEXPORT jlong JNICALL
Java_org_xmtp_reactnative_XmtpReactNativeModule_nativeInstall(JNIEnv *env,
                                                              jclass,
                                                              jlong pointer,
                                                              jobject holder) {
  try {
    auto local = facebook::jni::make_local(holder);
    auto reference = facebook::jni::static_ref_cast<
        facebook::react::CallInvokerHolder::javaobject>(local);
    auto &runtime = *reinterpret_cast<facebook::jsi::Runtime *>(pointer);
    auto owner =
        Runtime::install(runtime, reference->cthis()->getCallInvoker());
    return reinterpret_cast<jlong>(new Owner(std::move(owner)));
  } catch (...) {
    env->ThrowNew(env->FindClass("java/lang/IllegalStateException"),
                  "XMTP native runtime installation failed");
    return 0;
  }
}

extern "C" JNIEXPORT void JNICALL
Java_org_xmtp_reactnative_XmtpReactNativeModule_nativeStop(JNIEnv *env, jclass,
                                                           jlong pointer) {
  try {
    auto *owner = reinterpret_cast<Owner *>(pointer);
    (*owner)->stop();
  } catch (...) {
    env->ThrowNew(env->FindClass("java/lang/IllegalStateException"),
                  "XMTP native runtime stop failed");
  }
}

extern "C" JNIEXPORT void JNICALL
Java_org_xmtp_reactnative_XmtpReactNativeModule_nativeInvalidate(
    JNIEnv *env, jclass, jlong pointer) {
  try {
    std::unique_ptr<Owner> owner(reinterpret_cast<Owner *>(pointer));
    (*owner)->invalidate();
  } catch (...) {
    env->ThrowNew(env->FindClass("java/lang/IllegalStateException"),
                  "XMTP native runtime retirement failed");
  }
}

extern "C" JNIEXPORT jboolean JNICALL
Java_org_xmtp_reactnative_XmtpReactNativeModule_nativeSeedLifecycle(
    JNIEnv *, jclass, jboolean foreground) {
  return xmtp::reactnative::seedLifecycle(foreground);
}

extern "C" JNIEXPORT void JNICALL
Java_org_xmtp_reactnative_XmtpReactNativeModule_nativeSetForeground(
    JNIEnv *, jclass, jboolean foreground) {
  xmtp::reactnative::setForeground(foreground);
}
