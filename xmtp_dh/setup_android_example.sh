#!/bin/bash

set -e

PROJECT_PATH="../examples/android/xmtpv3_example"
mkdir -p $PROJECT_PATH/app/src/main/java/com/example/xmtpv3_example/

# Copy the jniLibs folder to the example project
rm -rf $PROJECT_PATH/app/src/main/jniLibs
cp -r jniLibs $PROJECT_PATH/app/src/main/

# Copy the .kt files to the example project
rm -f $PROJECT_PATH/app/src/main/java/xmtp_dh.kt
cp src/uniffi/xmtp_dh/xmtp_dh.kt $PROJECT_PATH/app/src/main/java/

# Copy MainActivity.kt to the example project (comment this out if copying to a different app)
rm -f $PROJECT_PATH/app/src/main/java/com/example/xmtpv3_example/MainActivity.kt
ln examples/MainActivity.kt $PROJECT_PATH/app/src/main/java/com/example/xmtpv3_example/MainActivity.kt

echo "Now open the example project at $PROJECT_PATH and build in Android Studio"
