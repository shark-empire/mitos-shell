#!/usr/local/bin/mitos

echo "Running script with $# arguments."
echo "First argument is: $1"

check_file() {
    if [ -f "$1" ]; then
        echo "File $1 exists!"
        return 0
    else
        echo "File $1 not found."
        return 1
    fi
}

check_file "Cargo.toml"

case "$1" in
    start)
        echo "Starting MITOS services..."
        ;;
    stop)
        echo "Stopping MITOS services..."
        ;;
    *)
        echo "Usage: $0 {start|stop}"
        ;;
esac
