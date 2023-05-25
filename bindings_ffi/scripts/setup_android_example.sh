#!/bin/bash

set -e

PROJECT_PATH="examples/android/xmtpv3_example"

# Copy the jniLibs folder to the example project
rm -rf $PROJECT_PATH/app/src/main/jniLibs
cp -r jniLibs $PROJECT_PATH/app/src/main/

# Copy the .kt files to the example project
rm -rf $PROJECT_PATH/app/src/main/java/xmtpv3.kt
cp src/uniffi/xmtpv3/xmtpv3.kt $PROJECT_PATH/app/src/main/java/

echo "Now open the example project at $PROJECT_PATH and build in Android Studio"
