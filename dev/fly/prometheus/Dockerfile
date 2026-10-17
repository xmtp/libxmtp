FROM prom/prometheus:v3.6.0
# Fly mounts the persistent volume as root.
USER 0
COPY prometheus.yaml /etc/prometheus/prometheus.yml
CMD ["--config.file=/etc/prometheus/prometheus.yml", "--storage.tsdb.path=/prometheus", "--storage.tsdb.retention.time=3d", "--storage.tsdb.retention.size=1GB", "--web.listen-address=[::]:9090", "--web.enable-remote-write-receiver", "--enable-feature=exemplar-storage"]
