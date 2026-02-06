#!/usr/bin/python
import os
import subprocess

if (input("\nAre you sure? This will replace the current \"release\" branch with origin/main. Y/n\n") != "Y"):
  print(u'\U0001f44C' + " lmk if you change your mind")
  exit(0)

os.system("git fetch origin")
os.system("git checkout release")

os.system("git reset --hard origin/main")
os.system("git push --force origin release")

print(u'\U0001f680' + " Successfully pushed origin/main to release")
print(u'\U0001f440' + " Follow the deploy at https://github.com/xmtp/xmtp-android/actions")
