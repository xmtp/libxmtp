#!/bin/bash

rm -rf dist
nj-cli build
node app.cjs
