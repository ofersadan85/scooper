FROM rust:alpine AS builder

RUN apk update && apk add --no-cache musl-dev
WORKDIR /build
COPY . .

RUN cargo build --release
RUN strip -g -S -d --strip-debug -s /build/target/release/scooper

FROM alpine:latest AS runtime

WORKDIR /app
COPY --from=builder /build/target/release/scooper /app/scooper

CMD ["./scooper"]
