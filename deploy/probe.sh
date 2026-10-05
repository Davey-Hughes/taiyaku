#!/bin/bash
# One GET to the server in this container, over IPv4 loopback, and whether the
# status it answered is the one expected. The image's HEALTHCHECK, and CI's
# probe of a freshly built image.
#
# `127.0.0.1`, never `localhost`: `serve --addr 0.0.0.0:8484` listens on IPv4
# only, and a resolver that tries `::1` first would report a healthy server as
# down. Bash's `/dev/tcp`, because the runtime image carries no curl or wget.
#
# Usage: taiyaku-probe PATH EXPECTED_STATUS
set -euo pipefail
path="${1:?a path, such as /}"
want="${2:?the status expected, such as 200}"
exec 3<>/dev/tcp/127.0.0.1/8484
printf 'GET %s HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n' "$path" >&3
read -r _ status _ <&3
[ "$status" = "$want" ]
