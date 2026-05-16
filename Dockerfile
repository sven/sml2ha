# Build stage
FROM rust:1.95.0-slim-trixie AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libudev-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/sml2ha

# Copy manifests and build dependencies to cache them
COPY Cargo.toml Cargo.lock ./
# Create a dummy source file to build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

# Copy the actual source code
COPY src ./src
# Touch the main file to ensure it gets rebuilt
RUN touch src/main.rs
RUN cargo build --release

# Runtime stage
FROM debian:trixie-slim

# Install runtime dependencies (libudev is often needed for serial communication)
RUN apt-get update && apt-get install -y \
    libudev1 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary from the builder stage
COPY --from=builder /usr/src/sml2ha/target/release/sml2ha /app/sml2ha

# Set the entrypoint to the application
ENTRYPOINT ["/app/sml2ha"]

# Default command specifies the configuration file location
CMD ["--config", "/app/config.yml"]
