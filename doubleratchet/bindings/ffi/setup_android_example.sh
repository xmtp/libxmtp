#!/bin/bash

set -e

PROJECT_NAME="xmtpv3_example"

# Copy the jniLibs folder to the example project
cp -r jniLibs ../../examples/$PROJECT_NAME/app/src/main/

# Copy the .kt files to the example project
cp src/uniffi/xmtpv3/xmtpv3.kt ../../examples/$PROJECT_NAME/app/src/main/java/

echo "Now open the example project at ../../examples/$PROJECT_NAME and build in Android Studio"
