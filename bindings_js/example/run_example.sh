#!/bin/bash

set -ex

rm -rf dist
pushd ..
rm -rf dist
npm run build
popd
cp -r ../dist dist

python3 -m http.server 9099 &
serverPID=$!
echo "Running tests in browser..."
open http://localhost:9099
sleep 5
kill $serverPID
