#!/usr/bin/env bash

for i in `seq $1 50 $2`;
    do
        ./target/release/evm-io-tracker fetch --node-url $NODE_URL --start-block $i --batch-size 50
    done