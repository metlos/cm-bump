#!/bin/sh

# prepare the location where cm-bump will store the configurations
mkdir -p /tmp/nginx-locations
mkdir -p /tmp/log/cm-bump
./cm-bump --dir=/tmp/nginx-locations > /tmp/log/cm-bump/cm-bump.log 2>&1 &

nginx -g 'daemon off;'
