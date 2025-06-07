FROM ubuntu:24.04
RUN apt-get update && apt-get install -y sqlite3 libssl-dev patchelf
COPY .cache/mls-validation-service /usr/local/bin/mls-validation-service
# in case the binary was created in a nix environment, does not effect non-nix builds (just replaces file paths)
RUN patchelf --set-interpreter /lib64/ld-linux-x86-64.so.2 /usr/local/bin/mls-validation-service
RUN patchelf --set-rpath "/lib:/usr/lib:/usr/local/lib" /usr/local/bin/mls-validation-service
RUN ldd /usr/local/bin/mls-validation-service
CMD ["mls-validation-service"]
