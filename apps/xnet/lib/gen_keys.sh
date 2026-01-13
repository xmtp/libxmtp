#!/bin/bash

# Generate 100 secp256k1 private keys for Ethereum testing
OUTPUT_FILE="signers.txt"

> "$OUTPUT_FILE"

for i in $(seq 1 100); do
    # Generate a secp256k1 key pair and extract the 32-byte private key as hex
    openssl ecparam -name secp256k1 -genkey -noout 2>/dev/null | \
    openssl ec -text -noout 2>/dev/null | \
    grep -A 3 "priv:" | tail -3 | tr -d ' :\n' >> "$OUTPUT_FILE"
    echo >> "$OUTPUT_FILE"
done

echo "Generated 100 secp256k1 private keys to $OUTPUT_FILE"
