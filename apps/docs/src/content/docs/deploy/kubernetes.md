---
title: Deploy on Kubernetes
description: Create a small Helm chart with Envoy Gateway and an external PostgreSQL database.
---

Create the chart files below in a directory outside the repository. Envoy Gateway
terminates TLS and sends h2c to the backend Service. Read the
[deployment overview](/deploy/overview/) for ports, connection budgets, health,
shutdown, the ingress contract, and security requirements.

**Local validation only.** This guide has not been installed on a real cluster
or exercised with a client. Helm and kubeconform check manifest structure. They
do not prove that the controller accepts the routes or preserves subscriptions.

**Image requirement:** use an image built from the configuration flag change in
commit `7d44488fe14ce903382a2b24c52cefe33c4f0e93` or later. Pin its published
`ghcr.io/xmtp/backend:sha-<commit>` tag. The published `:self-hosted` image at the
time of this check predates that change. It rejects `--config-file` at startup.
A real cluster install is blocked until a compatible image is published.

## Prerequisites

- **Kubernetes 1.34 or later.** Native gRPC probes are stable since 1.27.
  The `preStop.sleep` action is stable in 1.34 and enabled by default since 1.30.
  This guide uses the stable versions of both features.
- **Envoy Gateway v1.7.0**, with its Gateway API **v1.4.1** CRDs. This is the
  selected controller version. Its policy schemas are part of the local checks.
  Use a cluster that can provision a `LoadBalancer` Service for Envoy.
- **Helm 4.2.4** and **kubeconform 0.8.0** for the commands below, plus `kubectl`.
- **An external database** that meets the
  [database requirements](/deploy/overview/#database-and-migrations).
  The chart does not install PostgreSQL. Create an existing Secret named
  `xmtp-database` in namespace `xmtp`, with the database URL in key `url`.
  Use your secret manager or secure input from a file. Do not put the URL in
  Helm values, TOML, or command arguments.
- **A DNS name and certificate.** Obtain a trusted certificate and private key
  for `xmtp.example.com`. Keep the private key outside version control.
- **Access controls** that meet the [security requirements](/deploy/overview/#security).
  The minimal TOML below has no authentication. Restrict the Gateway load
  balancer to trusted clients before enabling the route, or configure `[auth]`.
- **Optional controllers:** the ServiceMonitor needs Prometheus Operator
  **v0.89.0** CRDs and a Prometheus instance that selects its labels. CPU
  autoscaling needs Metrics Server. Neither is installed by this chart.

Gateway API's h2c backend protocol is conformance-tested across Envoy Gateway,
Cilium, Istio, GKE, NGINX Gateway Fabric, and Traefik. The latter five are
**UNTESTED alternatives for this guide**. Conformance results do not verify this
backend deployment. See the [Gateway API conformance reports](https://gateway-api.sigs.k8s.io/implementations/).

## Create the chart

Create `xmtp-chart/templates`. Save each fence at the path in its title.
The default value set creates only a Deployment, ConfigMap, and private Service.
Replace the image tag before installation. `sha-REPLACE_WITH_FULL_COMMIT` is a
placeholder, not a published image.

```yaml title="xmtp-chart/Chart.yaml"
apiVersion: v2
name: xmtp-backend
description: XMTP backend with an external database and optional Envoy Gateway route
type: application
version: 0.1.0
kubeVersion: ">=1.34.0-0"
```

```yaml title="xmtp-chart/values.yaml"
image:
  repository: ghcr.io/xmtp/backend
  tag: sha-REPLACE_WITH_FULL_COMMIT
replicas: 1
databaseSecret:
  name: xmtp-database
  key: url
config: |
  [database]
  url = "env:XMTP_DATABASE_URL"
resources:
  requests:
    cpu: 250m
    memory: 256Mi
  limits:
    memory: 1Gi
gateway:
  enabled: false
  hostname: xmtp.example.com
  certificateSecret: xmtp-tls
serviceMonitor:
  enabled: false
  labels: {}
autoscaling:
  enabled: false
  minReplicas: 2
  maxReplicas: 4
  targetCPUUtilizationPercentage: 70
podDisruptionBudget:
  enabled: false
  minAvailable: 1
```

The ConfigMap holds no secrets. The checksum changes the pod template when the
configuration changes. The process mounts a file, so its arguments must use
`--config-file`. `--config` accepts inline TOML and is not correct here.

<!-- prettier-ignore -->
```yaml title="xmtp-chart/templates/configmap.yaml"
apiVersion: v1
kind: ConfigMap
metadata:
  name: {{ .Release.Name }}-config
data:
  config.toml: |
{{ .Values.config | indent 4 }}
```

<!-- prettier-ignore -->
```yaml title="xmtp-chart/templates/deployment.yaml"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ .Release.Name }}
spec:
  {{- if not .Values.autoscaling.enabled }}
  replicas: {{ .Values.replicas }}
  {{- end }}
  strategy:
    type: RollingUpdate
    rollingUpdate:
      maxUnavailable: 0
      maxSurge: 1
  selector:
    matchLabels:
      app.kubernetes.io/instance: {{ .Release.Name }}
  template:
    metadata:
      labels:
        app.kubernetes.io/instance: {{ .Release.Name }}
      annotations:
        checksum/config: {{ include (print $.Template.BasePath "/configmap.yaml") . | sha256sum | quote }}
    spec:
      automountServiceAccountToken: false
      terminationGracePeriodSeconds: 45
      securityContext:
        runAsNonRoot: true
        runAsUser: 10001
        runAsGroup: 10001
        seccompProfile:
          type: RuntimeDefault
      containers:
        - name: backend
          image: {{ printf "%s:%s" .Values.image.repository .Values.image.tag | quote }}
          args: ["--config-file", "/etc/xmtp/config.toml"]
          env:
            - name: XMTP_DATABASE_URL
              valueFrom:
                secretKeyRef:
                  name: {{ .Values.databaseSecret.name | quote }}
                  key: {{ .Values.databaseSecret.key | quote }}
          ports:
            - name: grpc
              containerPort: 5050
            - name: metrics
              containerPort: 9464
          securityContext:
            allowPrivilegeEscalation: false
            readOnlyRootFilesystem: true
            capabilities:
              drop: ["ALL"]
          resources:
{{ toYaml .Values.resources | indent 12 }}
          startupProbe:
            grpc:
              port: 5050
            periodSeconds: 5
            timeoutSeconds: 2
            failureThreshold: 120
          readinessProbe:
            grpc:
              port: 5050
            periodSeconds: 5
            timeoutSeconds: 2
          livenessProbe:
            grpc:
              port: 5050
            periodSeconds: 10
            timeoutSeconds: 2
            failureThreshold: 3
          lifecycle:
            preStop:
              sleep:
                seconds: 15
          volumeMounts:
            - name: config
              mountPath: /etc/xmtp
              readOnly: true
      volumes:
        - name: config
          configMap:
            name: {{ .Release.Name }}-config
```

`startupProbe` allows 120 failures at 5 s intervals: about **600 s** for startup.
Increase this budget if migrations need more time. In `apps/backend/src/main.rs`,
`run` awaits `server::initialize(config)` before `TcpListener::bind`.
Initialization finishes primary database migrations before it returns. The RPC
port cannot pass a probe until that work finishes. Kubernetes delays readiness
and liveness probes until the startup probe succeeds.

The image has no shell. Use native `grpc:` probes and `preStop.sleep`, with no
exec command. The 15 s sleep gives endpoint changes time to reach the Gateway
before SIGTERM. This delay needs a real rollout test; it is not a guarantee.

The sleep runs **inside** the termination grace period. The source defaults are
`DEFAULT_DRAIN_DURATION_MS = 10_000` in `apps/backend/src/config.rs` and
`OTLP_FLUSH_TIMEOUT = 5 s` in `crates/xmtp_logging/src/telemetry.rs`.
The chart therefore uses **45 s = 15 s sleep + 10 s drain + 5 s flush + 15 s margin**.
A 30 s grace period would leave no margin. Increase it if you increase the sleep
or the configured drain. See [shutdown](/deploy/overview/#shutdown) for client
reconnect behavior.

<!-- prettier-ignore -->
```yaml title="xmtp-chart/templates/service.yaml"
apiVersion: v1
kind: Service
metadata:
  name: {{ .Release.Name }}
  labels:
    app.kubernetes.io/instance: {{ .Release.Name }}
spec:
  type: ClusterIP
  selector:
    app.kubernetes.io/instance: {{ .Release.Name }}
  ports:
    - name: grpc
      port: 5050
      targetPort: grpc
      appProtocol: kubernetes.io/h2c
    - name: metrics
      port: 9464
      targetPort: metrics
```

## Gateway and TLS

Use an HTTPRoute for the whole hostname. GRPCRoute does not cover HTTP/1.1
gRPC-Web or its CORS preflight. An HTTPRoute and a GRPCRoute with the same
hostname conflict under the Gateway API rules. Do not add a GRPCRoute beside
this route. `appProtocol: kubernetes.io/h2c` selects HTTP/2 to the backend for
both client transports.

The GatewayClass is cluster-scoped. Its name includes the release namespace and
name so separate releases do not claim the same class.

<!-- prettier-ignore -->
```yaml title="xmtp-chart/templates/gateway.yaml"
{{- if .Values.gateway.enabled }}
apiVersion: gateway.networking.k8s.io/v1
kind: GatewayClass
metadata:
  name: {{ .Release.Namespace }}-{{ .Release.Name }}
spec:
  controllerName: gateway.envoyproxy.io/gatewayclass-controller
---
apiVersion: gateway.networking.k8s.io/v1
kind: Gateway
metadata:
  name: {{ .Release.Name }}
spec:
  gatewayClassName: {{ .Release.Namespace }}-{{ .Release.Name }}
  listeners:
    - name: https
      hostname: {{ .Values.gateway.hostname | quote }}
      port: 443
      protocol: HTTPS
      tls:
        mode: Terminate
        certificateRefs:
          - group: ""
            kind: Secret
            name: {{ .Values.gateway.certificateSecret | quote }}
      allowedRoutes:
        namespaces:
          from: Same
---
apiVersion: gateway.networking.k8s.io/v1
kind: HTTPRoute
metadata:
  name: {{ .Release.Name }}
spec:
  parentRefs:
    - name: {{ .Release.Name }}
      sectionName: https
  hostnames:
    - {{ .Values.gateway.hostname | quote }}
  rules:
    - matches:
        - path:
            type: PathPrefix
            value: /
      timeouts:
        request: 0s
        backendRequest: 0s
      backendRefs:
        - name: {{ .Release.Name }}
          port: 5050
---
apiVersion: gateway.envoyproxy.io/v1alpha1
kind: BackendTrafficPolicy
metadata:
  name: {{ .Release.Name }}
spec:
  targetRefs:
    - group: gateway.networking.k8s.io
      kind: HTTPRoute
      name: {{ .Release.Name }}
  timeout:
    http:
      requestTimeout: 0s
      maxStreamDuration: 0s
---
apiVersion: gateway.envoyproxy.io/v1alpha1
kind: ClientTrafficPolicy
metadata:
  name: {{ .Release.Name }}
spec:
  targetRefs:
    - group: gateway.networking.k8s.io
      kind: Gateway
      name: {{ .Release.Name }}
  tls:
    alpnProtocols: ["h2", "http/1.1"]
  timeout:
    http:
      requestReceivedTimeout: 0s
      streamIdleTimeout: 0s
{{- end }}
```

These are Envoy Gateway v1.7.0 timeout fields. `requestReceivedTimeout` removes
the client request read deadline. `streamIdleTimeout` removes the stream
inactivity deadline in either direction, including response sends.
The route timeouts and backend policy remove the upstream response deadline
and maximum stream duration. Here `0s` disables each limit. This permits long
subscriptions, including quiet periods. Apply the shared
[security controls](/deploy/overview/#security) because idle streams can retain
resources. No body buffering, gRPC conversion, CORS rewrite, or retry policy is
added. All [ingress checks](/deploy/overview/#ingress-contract) still need a client test.

Create the certificate Secret separately from Helm. The required manifest has
this shape. Replace both placeholders with PEM data from your certificate
provider before applying it. `tls.crt` must contain the certificate chain.
Do not commit the populated file or send it to Helm as a value.

```yaml title="certificate.yaml"
apiVersion: v1
kind: Secret
metadata:
  name: xmtp-tls
  namespace: xmtp
type: kubernetes.io/tls
stringData:
  tls.crt: |
    REPLACE_WITH_PEM_CERTIFICATE_CHAIN
  tls.key: |
    REPLACE_WITH_PEM_PRIVATE_KEY
```

## Optional monitoring and scaling

<!-- prettier-ignore -->
```yaml title="xmtp-chart/templates/optional.yaml"
{{- if .Values.serviceMonitor.enabled }}
apiVersion: monitoring.coreos.com/v1
kind: ServiceMonitor
metadata:
  name: {{ .Release.Name }}
  labels:
{{ toYaml .Values.serviceMonitor.labels | indent 4 }}
spec:
  selector:
    matchLabels:
      app.kubernetes.io/instance: {{ .Release.Name }}
  endpoints:
    - port: metrics
      path: /metrics
      interval: 30s
{{- end }}
{{- if .Values.autoscaling.enabled }}
---
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: {{ .Release.Name }}
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: {{ .Release.Name }}
  minReplicas: {{ .Values.autoscaling.minReplicas }}
  maxReplicas: {{ .Values.autoscaling.maxReplicas }}
  metrics:
    - type: Resource
      resource:
        name: cpu
        target:
          type: Utilization
          averageUtilization: {{ .Values.autoscaling.targetCPUUtilizationPercentage }}
{{- end }}
{{- if .Values.podDisruptionBudget.enabled }}
---
apiVersion: policy/v1
kind: PodDisruptionBudget
metadata:
  name: {{ .Release.Name }}
spec:
  minAvailable: {{ .Values.podDisruptionBudget.minAvailable }}
  selector:
    matchLabels:
      app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}
```

Save this second value set beside the chart. Match `serviceMonitor.labels` to
your Prometheus selector. The monitor selects Services in its own namespace.
The Gateway exposes only the application port. Apply network policy or cluster
network controls to keep the metrics port limited to monitoring clients.

```yaml title="enabled.yaml"
gateway:
  enabled: true
serviceMonitor:
  enabled: true
  labels:
    release: prometheus
autoscaling:
  enabled: true
podDisruptionBudget:
  enabled: true
```

The CPU request enables utilization-based scaling. CPU is only a starting
signal; validate capacity with subscription load. Size the database for
`maxReplicas` plus the rolling update surge, using the shared
[connection budget](/deploy/overview/#connection-budget). Terminating pods can
keep connections during the grace period, so reserve capacity for them too.
The PDB limits voluntary evictions. It does not control Deployment rollouts or
prevent node failures. With one replica, `minAvailable: 1` blocks an eviction.
Use at least two replicas when you enable it.

## Install

These commands are instructions for a future live check. They were not run
against a cluster. First publish a compatible image, set its immutable tag in
`values.yaml`, and prepare the database Secret and access controls.

Install the pinned controller, which includes its Gateway API and Envoy policy CRDs:

```sh
helm install eg oci://docker.io/envoyproxy/gateway-helm \
  --version v1.7.0 --namespace envoy-gateway-system --create-namespace \
  --wait=watcher --timeout 10m
kubectl create namespace xmtp
kubectl apply -f certificate.yaml
```

Use a certificate renewal process that updates `xmtp-tls` before expiry.
For an existing namespace, omit `kubectl create namespace`. Create the database
Secret there before the next command. Install Prometheus Operator and Metrics
Server before using `enabled.yaml`; otherwise enable only the Gateway with
`--set gateway.enabled=true` in place of `-f enabled.yaml`.

```sh
helm upgrade --install xmtp ./xmtp-chart --namespace xmtp \
  -f enabled.yaml --rollback-on-failure --wait=watcher --timeout 15m
kubectl -n xmtp rollout status deployment/xmtp --timeout=15m
kubectl -n xmtp get gateway xmtp
kubectl -n xmtp get httproute xmtp -o yaml
kubectl -n xmtp get backendtrafficpolicy,clienttrafficpolicy
```

These commands use **Helm 4** spellings: `--rollback-on-failure` replaces
`--atomic`, and `--wait=watcher` names the wait strategy. The 15-minute Helm
budget exceeds the startup probe budget. Check Gateway `Accepted` and
`Programmed`, route `Accepted` and `ResolvedRefs`, and policy `Accepted`
conditions. Helm readiness alone does not prove that the route works.

Point the DNS name at the Gateway address. Use `https://xmtp.example.com` as the
SDK backend URL. For a configuration edit, change `values.yaml` and repeat the
upgrade command. The checksum causes a rollout. An external Secret change does
not change the checksum; restart the Deployment to refresh its environment.
Check [migration limits](/deploy/overview/#database-and-migrations) before any
image upgrade.

## Local manifest checks

Run these checks from the directory that contains `xmtp-chart` and
`enabled.yaml`. They need no cluster. Use Python 3 with PyYAML to create local
JSON schemas from the pinned upstream CRDs:

```python title="prepare-schemas.py"
import json
from pathlib import Path
from urllib.request import urlopen

import yaml

sources = {
    "gateway.networking.k8s.io": (
        "https://raw.githubusercontent.com/kubernetes-sigs/gateway-api/"
        "v1.4.1/config/crd/standard/",
        ["gatewayclasses", "gateways", "httproutes"],
    ),
    "gateway.envoyproxy.io": (
        "https://raw.githubusercontent.com/envoyproxy/gateway/"
        "v1.7.0/charts/gateway-helm/crds/generated/",
        ["backendtrafficpolicies", "clienttrafficpolicies"],
    ),
    "monitoring.coreos.com": (
        "https://raw.githubusercontent.com/prometheus-operator/prometheus-operator/"
        "v0.89.0/example/prometheus-operator-crd/",
        ["servicemonitors"],
    ),
}


def strict(schema):
    if isinstance(schema, dict):
        if "properties" in schema and "additionalProperties" not in schema:
            schema["additionalProperties"] = False
        for value in schema.values():
            strict(value)
    elif isinstance(schema, list):
        for value in schema:
            strict(value)


for group, (base, resources) in sources.items():
    for resource in resources:
        with urlopen(f"{base}{group}_{resource}.yaml") as response:
            crd = yaml.safe_load(response)
        for version in crd["spec"]["versions"]:
            if not version["served"]:
                continue
            schema = version["schema"]["openAPIV3Schema"]
            strict(schema)
            # Kubernetes supplies metadata outside the custom resource schema.
            schema["properties"]["metadata"] = {"type": "object"}
            kind = crd["spec"]["names"]["kind"].lower()
            path = Path("schemas") / group / f"{kind}_{version['name']}.json"
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(json.dumps(schema))
```

An unreachable source fails this script. Do not skip missing schemas. Run it,
then check both named value sets with Kubernetes 1.34 schemas:

```sh
python3 prepare-schemas.py
helm lint --strict ./xmtp-chart
helm lint --strict ./xmtp-chart -f enabled.yaml
```

Use Bash with `pipefail` so a failed render also fails the pipeline:

```bash
set -o pipefail
helm template xmtp ./xmtp-chart --namespace xmtp --kube-version 1.34.0 | \
  kubeconform -strict -summary -kubernetes-version 1.34.0 \
  -schema-location default \
  -schema-location 'schemas/{{.Group}}/{{.ResourceKind}}_{{.ResourceAPIVersion}}.json'
helm template xmtp ./xmtp-chart --namespace xmtp --kube-version 1.34.0 \
  -f enabled.yaml | \
  kubeconform -strict -summary -kubernetes-version 1.34.0 \
  -schema-location default \
  -schema-location 'schemas/{{.Group}}/{{.ResourceKind}}_{{.ResourceAPIVersion}}.json'
```

## Verification status and cleanup

Local validation used **Helm 4.2.4** and **kubeconform 0.8.0**, from
`nix shell nixpkgs#kubernetes-helm nixpkgs#kubeconform`. The chart was extracted
from this page's fences into a scratch directory outside the repository.
The named value sets are **defaults** and **enabled**. The enabled set includes
the Gateway route, both Envoy policies, ServiceMonitor, autoscaling, and PDB.

The schema sources are pinned to Gateway API v1.4.1, Envoy Gateway v1.7.0, and
Prometheus Operator v0.89.0. Each CRD's `openAPIV3Schema` is used for its served
version. Schema validation does not evaluate Kubernetes CEL rules, references,
certificate contents, or controller behavior. The certificate fence contains
placeholders and is not a usable certificate.

Both `helm lint --strict` runs reported:

```text
1 chart(s) linted, 0 chart(s) failed
```

The defaults pipeline reported:

```text
Summary: 3 resources found parsing stdin - Valid: 3, Invalid: 0, Errors: 0, Skipped: 0
```

The enabled pipeline reported:

```text
Summary: 11 resources found parsing stdin - Valid: 11, Invalid: 0, Errors: 0, Skipped: 0
```

The exact `-schema-location` values were `default` and
`schemas/{{.Group}}/{{.ResourceKind}}_{{.ResourceAPIVersion}}.json`.
All CRD sources were reachable. No resource was skipped.

**No real cluster or client verification has passed.** The known published image
rejects `--config-file` with `error: unexpected argument '--config-file' found`.
No cluster install was attempted for this guide. Local checks are not equivalent
to deployment verification.

To close the gap after publishing the required image:

1. Install this chart on a real cluster with the pinned controller and image.
   Confirm probe, Gateway, route, policy, and certificate status.
2. Run xdbg against the Gateway hostname. Register identities, create a group,
   publish messages, and query them back.
3. Use `grpcurl -vv` against that hostname to inspect trailers. Complete the
   shared [ingress checks](/deploy/overview/#ingress-contract), including
   HTTP/1.1 gRPC-Web, CORS preflight, and incremental response frames.
4. Keep a subscription open and run `kubectl -n xmtp rollout restart deployment/xmtp`.
   Confirm that endpoint propagation and `preStop.sleep` prevent a premature
   subscription drop. Confirm graceful shutdown, trailers, client reconnect,
   and delivery of later messages. Subscriptions end at backend shutdown by
   design; the check must distinguish that end from a forced connection cut.
   This is the required evidence for the 15 s sleep and 45 s grace period.

Remove test resources when the check is complete:

```sh
helm uninstall xmtp --namespace xmtp
kubectl -n xmtp delete secret xmtp-tls
helm uninstall eg --namespace envoy-gateway-system
```

Uninstall the controller only if this test owns it. Confirm that its cloud load
balancer is gone. Helm can leave CRDs behind. The external database, database
Secret, DNS record, and certificate renewal process are not chart resources.
Remove disposable test resources separately; retain production data.

As a legacy footnote, ingress-nginx is retired. Its best-effort maintenance
ended in March 2026. Do not use it for a new installation. See the
[retirement notice](https://kubernetes.io/blog/2025/11/11/ingress-nginx-retirement/).
