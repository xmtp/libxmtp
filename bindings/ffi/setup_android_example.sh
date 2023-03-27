#!/bin/bash

set -e

# Copy the jniLibs folder to the example project
cp -r jniLibs ../../examples/corecrypto_android_example/app/src/main/

# Copy the .kt files to the example project
cp src/uniffi/corecrypto/corecrypto.kt ../../examples/corecrypto_android_example/app/src/main/java/

echo "Now open the example project at ../../examples/corecrypto_android_example and build in Android Studio"
