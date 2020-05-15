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
COPY root.conf /etc/nginx/conf.d/root.conf

USER nginx

CMD ["./entrypoint.sh"]

