# /etc/profile.d/mitos.sh
if [ "$SHELL" = "/usr/bin/mitos" ]; then
    export PATH="/usr/local/bin:$PATH"
fi
