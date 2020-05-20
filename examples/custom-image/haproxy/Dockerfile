FROM rust:buster as build

WORKDIR usr/src/cm-bump
COPY .src .
RUN cargo install --path .

FROM haproxy:2.1 
COPY --from=build /usr/local/cargo/bin/cm-bump .
COPY entrypoint.sh .
RUN chmod a+x entrypoint.sh

# redirect the default haproxy config location to a place
# writable by cm-bump
RUN rm -Rf /usr/local/etc/haproxy
RUN ln -s /tmp/haproxy-config /usr/local/etc/haproxy

ENTRYPOINT ["./entrypoint.sh"]

