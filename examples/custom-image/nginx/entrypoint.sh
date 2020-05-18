#!/bin/bash

# First we need to finalize the configuration of our server.
# We need to put in the resolver address and DNS name validity period.

RESOLVER=`cat /etc/resolv.conf | grep nameserver | cut -d' ' -f1 --complement`
DNS_TTL=${DNS_TTL:-1m}

sed -i -E "s/\\{\\{RESOLVER\\}\\}/$RESOLVER/g" /tmp/nginx/root.conf
sed -i -E "s/\\{\\{DNS_TTL\\}\\}/$DNS_TTL/g" /tmp/nginx/root.conf

# prepare the location where cm-bump will store the configurations
mkdir /tmp/nginx/locations

mkdir -p /tmp/log/cm-bump
./cm-bump --dir=/tmp/nginx/locations > /tmp/log/cm-bump/cm-bump.log 2>&1 &

nginx -g 'daemon off;'
