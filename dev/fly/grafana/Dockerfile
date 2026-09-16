FROM grafana/grafana:12.2.0
# The mounted Fly volume is owned by root.
USER 0
COPY datasources.yaml /etc/grafana/provisioning/datasources/fly.yaml
COPY dashboards.yaml /etc/grafana/provisioning/dashboards/fly.yaml
COPY backend.json /etc/grafana/dashboards/backend.json
