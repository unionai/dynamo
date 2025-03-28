FROM --platform=${BUILDPLATFORM} golang:1.23-alpine AS builder

ARG TARGETARCH
ENV GOARCH="${TARGETARCH}"
ENV GOOS=linux
ENV CGO_ENABLED=0

WORKDIR /build

# Copy go mod files
COPY go.mod go.sum ./
RUN go mod download

# Copy source code
COPY . .

# Build the manager binary
RUN go build -ldflags="-w -s" -o manager ./cmd/main.go

# Final stage
FROM gcr.io/distroless/static-debian11

WORKDIR /
COPY --from=builder /build/manager .

USER 65532:65532
ENTRYPOINT ["/manager"]
