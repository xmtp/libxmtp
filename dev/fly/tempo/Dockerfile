FROM grafana/tempo:3.0.1
# The mounted Fly volume is owned by root.
USER 0
COPY tempo.yaml /etc/tempo.yaml
CMD ["-config.file=/etc/tempo.yaml"]
