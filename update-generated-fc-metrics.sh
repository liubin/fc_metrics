#!/bin/bash

set -e

if [ "$#" -ne 1 ]; then
    echo "Usage:  ${0} <metrics.rs_URL>, for example: ${0} https://github.com/firecracker-microvm/firecracker/blob/master/src/logger/src/metrics.rs#L255-L688"
    exit
fi

BASEDIR=$(dirname "$0")
path=$(readlink -m $BASEDIR)

pushd $path

## step 1: build fc-metrics-generator.
prog="fc-metrics-generator"
echo "building $prog"
cargo build

## step 2: download metrics.rs from firecracker's repo.
url=$1
echo "downloading $url"

local_file="target/metrics.rs"
curl -s $1 --output $local_file
echo "$url is saved to $local_file"

## step 3: generate and format golang source code
echo "generating $local_file"
target_file="$GOPATH/src/github.com/kata-containers/kata-containers/src/runtime/virtcontainers/fc_metrics.go"
./target/debug/$prog $local_file > $target_file

go fmt $target_file

popd

echo -e "\033[0;32mgenerated file saved to $target_file\033[0m"
