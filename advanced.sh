#!/usr/local/bin/mitos

# 1. Arrays
SERVERS=(web01 web02 db01)
echo "First server: ${SERVERS[0]}"
echo "All servers: ${SERVERS[@]}"
echo "Count: ${#SERVERS[@]}"

# 2. Loop over array
for server in "${SERVERS[@]}"; do
    echo "Pinging $server..."
done

# 3. Here-Document (Generate an Nginx config)
cat <<EOF > /tmp/nginx.conf
server {
    listen 80;
    server_name ${SERVERS[0]}.mitos.local;
    root /var/www/html;
}
EOF

echo "Config generated!"
cat /tmp/nginx.conf
