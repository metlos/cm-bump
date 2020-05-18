FROM rust:buster as build

WORKDIR usr/src/cm-bump
COPY .src .
RUN cargo install --path .

FROM nginxinc/nginx-unprivileged:stable
COPY --from=build /usr/local/cargo/bin/cm-bump .
COPY entrypoint.sh .
RUN chmod a+x entrypoint.sh

# modify the nginx configuration
USER root
RUN rm /etc/nginx/conf.d/*
COPY root.conf /tmp/nginx/root.conf
RUN ln -s /tmp/nginx/root.conf /etc/nginx/conf.d/root.conf

# The configuration needs to be modified in entrypoint.sh, so we need
# to make it writable.
RUN chmod a+rw /tmp/nginx/root.conf
RUN chmod a+rwx /tmp/nginx
 
USER nginx

CMD ["./entrypoint.sh"]

