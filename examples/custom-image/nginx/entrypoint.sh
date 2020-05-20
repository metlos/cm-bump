#!/bin/bash

# First we need to finalize the configuration of our server.
# We need to put in the resolver address and DNS name validity period.

RESOLVER=`cat /etc/resolv.conf | grep nameserver | head -1 | cut -d' ' -f1 --complement`
DNS_TTL=${DNS_TTL:-1m}

sed -i -E "s/\\{\\{RESOLVER\\}\\}/$RESOLVER/g" /tmp/nginx/root.conf
sed -i -E "s/\\{\\{DNS_TTL\\}\\}/$DNS_TTL/g" /tmp/nginx/root.conf

# prepare the location where cm-bump will store the configurations
mkdir /tmp/nginx/locations

mkdir -p /tmp/log/cm-bump
# We're starting cm bump and setting it up to send the signal configured from the environment
# to the current process PID. Later we start nginx using exec making it reuse our current PID
./cm-bump --dir=/tmp/nginx/locations --process-pid=$$ > /tmp/log/cm-bump/cm-bump.log 2>&1 &

exec nginx -g 'daemon off;'
