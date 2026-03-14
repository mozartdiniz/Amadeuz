#!/bin/bash
set -e

# Amadeuz Server Setup Script for Raspberry Pi
# This script installs Go (if missing) and prepares the environment.

echo "--- Amadeuz Server Setup ---"

# 1. Detect architecture
ARCH=$(uname -m)
case "$ARCH" in
    x86_64) GOARCH="amd64" ;;
    aarch64) GOARCH="arm64" ;;
    armv7l) GOARCH="armv6l" ;; # Pi Zero/1/2/3/4 (32-bit)
    *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

# 2. Install Go if not present or too old
GO_REQUIRED_VERSION="1.23.0"
if ! command -v go &> /dev/null || [[ "$(go version | awk '{print $3}' | sed 's/go//')" < "$GO_REQUIRED_VERSION" ]]; then
    echo "Installing/Updating Go to $GO_REQUIRED_VERSION ($GOARCH)..."
    
    # Download latest Go
    GO_TAR="go${GO_REQUIRED_VERSION}.linux-${GOARCH}.tar.gz"
    curl -LO "https://go.dev/dl/${GO_TAR}"
    
    # Remove old installation if exists and install new one
    sudo rm -rf /usr/local/go
    sudo tar -C /usr/local -xzf "${GO_TAR}"
    rm "${GO_TAR}"
    
    # Update PATH for current session and future ones
    export PATH=$PATH:/usr/local/go/bin
    if ! grep -q "/usr/local/go/bin" ~/.bashrc; then
        echo 'export PATH=$PATH:/usr/local/go/bin' >> ~/.bashrc
        echo 'export PATH=$PATH:$(go env GOPATH)/bin' >> ~/.bashrc
    fi
    echo "Go installed successfully."
else
    echo "Go $(go version | awk '{print $3}') is already installed."
fi

# 3. Prepare directories
echo "Preparing storage directories..."
mkdir -p blobs
chmod 700 blobs

# 4. Download dependencies
echo "Downloading Go modules..."
go mod download

# 5. Build (optional but recommended to verify)
echo "Building server binary..."
go build -o amadeuz-server .

echo "------------------------------------------------"
echo "Setup complete!"
echo "You can now run the server using:"
echo "  go run ."
echo ""
echo "Or use the compiled binary:"
echo "  ./amadeuz-server"
echo ""
echo "The server will listen on http://localhost:8080"
echo "------------------------------------------------"
