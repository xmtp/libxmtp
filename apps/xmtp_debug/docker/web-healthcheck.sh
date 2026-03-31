#!/bin/bash


: "${HEALTHCHECK_TIMEOUT:=10}"

function log {
    echo "[$(date '+%F %T')] [healthcheck] $*"
}

if [ -z "${PUSHGATEWAY_URL:-}" ]; then
    log "PUSHGATEWAY_URL not set, skipping metric push (health checks still run)"
fi

if [ -z "${WEB_HEALTHCHECK_ENDPOINTS}" ]; then
    log "WEB_HEALTHCHECK_ENDPOINTS not set, skipping health checks"
    exit 0
fi

IFS=',' read -ra ENDPOINTS <<< "${WEB_HEALTHCHECK_ENDPOINTS}"

if [ ${#ENDPOINTS[@]} -eq 0 ]; then
    log "No endpoints configured"
    exit 0
fi

log "Checking ${#ENDPOINTS[@]} endpoint(s)..."

for endpoint in "${ENDPOINTS[@]}"; do
    endpoint=$(echo "$endpoint" | xargs)

    endpoint_id=$(echo "$endpoint" | sed 's|https\?://||' | sed 's|[^a-zA-Z0-9]|_|g')

    log "Checking endpoint: $endpoint (id: $endpoint_id)"

    http_code=$(curl -o /dev/null -s -w "%{http_code}" -m "$HEALTHCHECK_TIMEOUT" "$endpoint")
    curl_exit_code=$?

    if [ $curl_exit_code -eq 0 ] && [ "$http_code" = "200" ]; then
        health_status=1
        log "[OK] $endpoint - OK (200)"
    else
        health_status=0
        if [ $curl_exit_code -ne 0 ]; then
            log "[FAIL] $endpoint - FAILED (curl error: $curl_exit_code)"
        else
            log "[FAIL] $endpoint - FAILED (HTTP $http_code)"
        fi
    fi

    metric_name="web_endpoint_health"

    metrics_payload="# HELP ${metric_name} Health status of web endpoints (1 = healthy, 0 = unhealthy)
# TYPE ${metric_name} gauge
${metric_name}{endpoint=\"${endpoint}\",endpoint_id=\"${endpoint_id}\",http_code=\"${http_code}\"} ${health_status}
"

    if [ -n "${PUSHGATEWAY_URL:-}" ]; then
        push_url="${PUSHGATEWAY_URL}/metrics/job/web_healthcheck/instance/${endpoint_id}"

        push_response=$(curl -s -w "\nHTTP_CODE:%{http_code}" -X POST -H "Content-Type: text/plain" --data-binary "$metrics_payload" "$push_url" 2>&1)
        push_exit_code=$?
        push_http_code=$(echo "$push_response" | grep "HTTP_CODE:" | cut -d: -f2)

        if [ "$push_exit_code" -eq 0 ] && [ "$push_http_code" = "200" ]; then
            log "Metrics pushed for $endpoint_id"
        else
            log "ERROR: Failed to push metrics for $endpoint_id (HTTP $push_http_code, exit code: $push_exit_code)"
            error_message=$(echo "$push_response" | grep -v "HTTP_CODE:")
            if [ -n "$error_message" ]; then
                log "ERROR: $error_message"
            fi
        fi
    fi
done

log "Health check cycle complete"
