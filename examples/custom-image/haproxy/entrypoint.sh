#!/bin/bash

# prepare the location where cm-bump will store the configurations
mkdir -p /tmp/haproxy-config

mkdir -p /tmp/log/cm-bump

# We're starting cm bump and setting it up to send the signal configured from the environment
# to the current process PID. Later we start haproxy using exec making it reuse our current PID
./cm-bump --dir=/tmp/haproxy-config --process-pid=$$ > /tmp/log/cm-bump/cm-bump.log 2>&1 &

# This seems to be necessary because it looks like haproxy doesn't react to sighup during the
# startup. It may therefore happen that haproxy reads old config but doesn't react to the
# new config. Sleeping doesn't necessarily solve that problem but gives time to cm-bump to 
# do the initial sync.
echo Sleeping for 3s before starting haproxy
sleep 3

set -- "$@" -f /tmp/haproxy-config

exec ./docker-entrypoint.sh $@
