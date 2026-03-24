#!/bin/sh
set -e

# Bind to all interfaces inside the container.
# No-op if the user mounted a config with a custom host.
if grep -q 'host: "127.0.0.1"' /app/config.yml; then
    sed -i 's/host: "127.0.0.1"/host: "0.0.0.0"/' /app/config.yml
fi

exec "$@"
