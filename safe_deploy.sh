#!/usr/local/bin/mitos
set -e  # Exit if any command fails
set -u  # Error if variables are missing
set -x  # Print commands as they run

# Trap Ctrl+C to clean up
trap 'echo "Interrupted! Cleaning up..."; rm -rf /tmp/build_*' INT TERM

echo "Starting deployment..."
BUILD_DIR="/tmp/build_$$"
mkdir "$BUILD_DIR"

# This will fail if $TARGET_SERVER is not set (thanks to set -u)
scp app.bin "$TARGET_SERVER:/opt/app/" 

# This will trigger errexit if scp fails, skipping the echo below
echo "Deployment successful!"
