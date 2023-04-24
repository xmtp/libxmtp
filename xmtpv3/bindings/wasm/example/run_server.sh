#!/bin/bash

set -ex

rm -rf dist
pushd ..
rm -rf dist
npm run build
popd
cp -r ../dist dist

python -m http.server 9099
