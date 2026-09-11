#!/usr/bin/env bash

for i in `seq 0 1000000 25700000`;
    do
        ./target/release/evm-io-tracker sort-accounts --node-url $NODE_URL --start-block 1 --end-block $i --batch-size 200
    done

./target/release/evm-io-tracker sort-accounts --node-url $NODE_URL --start-block 1 --end-block 25700000 --batch-size 200
