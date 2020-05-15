#!/bin/sh

DEFAULT_IMAGE=quay.io/lkrejci/cm-bump-nginx-prototype:latest
IMAGE=${1:-$DEFAULT_IMAGE}

mkdir -p .src
SRCS=../../..
cp -R $SRCS/Cargo.toml $SRCS/Cargo.lock $SRCS/src .src

docker build -t $IMAGE .

