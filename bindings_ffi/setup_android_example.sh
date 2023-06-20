#!/bin/bash

set -e

APP_PATH="../examples/android/xmtpv3_example"
PROJECT_NAME="xmtpv3"
mkdir -p $APP_PATH/app/src/main/java/com/example/xmtpv3_example/

# Copy the jniLibs folder to the example project
rm -rf $APP_PATH/app/src/main/jniLibs
cp -r jniLibs $APP_PATH/app/src/main/

# Copy the .kt files to the example project
rm -f $APP_PATH/app/src/main/java/$PROJECT_NAME.kt
cp src/uniffi/$PROJECT_NAME/$PROJECT_NAME.kt $APP_PATH/app/src/main/java/

# Copy MainActivity.kt and ExampleInstrumentedTest.kt to the example project (comment this out if copying to a different app)
rm -f $APP_PATH/app/src/main/java/com/example/xmtpv3_example/MainActivity.kt
ln examples/MainActivity.kt $APP_PATH/app/src/main/java/com/example/xmtpv3_example/MainActivity.kt
rm -f $APP_PATH/app/src/androidTest/java/com/example/xmtpv3_example/ExampleInstrumentedTest.kt
ln examples/ExampleInstrumentedTest.kt $APP_PATH/app/src/androidTest/java/com/example/xmtpv3_example/ExampleInstrumentedTest.kt

echo "Now open the example project at $APP_PATH and build in Android Studio"
