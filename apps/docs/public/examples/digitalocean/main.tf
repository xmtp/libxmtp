terraform {
  required_version = ">= 1.7.0"
  required_providers {
    digitalocean = {
      source  = "digitalocean/digitalocean"
      version = "~> 2.106.0"
    }
    random = {
      source  = "hashicorp/random"
      version = "~> 3.9.1"
    }
    restapi = {
      source  = "Mastercard/restapi"
      version = "~> 3.0.0"
    }
  }
}

variable "digitalocean_token" {
  type      = string
  sensitive = true
}

variable "name" {
  type    = string
  default = "xmtp-backend"
}

variable "server_identifier" {
  description = "A reverse-DNS name you control. Keep it unchanged after clients connect."
  type        = string
}

variable "backend_image" {
  description = "Public XMTP backend image, pinned to a release, commit tag, or digest."
  type        = string
  default     = "ghcr.io/xmtp/backend:8.0.0-rc4"

  validation {
    condition     = can(regex("^ghcr\\.io/xmtp/backend(:[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}|@sha256:[0-9a-f]{64})$", var.backend_image))
    error_message = "Use ghcr.io/xmtp/backend:<tag> or ghcr.io/xmtp/backend@sha256:<digest>."
  }
}

variable "r2_endpoint" {
  description = "https://<ACCOUNT_ID>.r2.cloudflarestorage.com, without the bucket name."
  type        = string
}

variable "r2_bucket" {
  type = string
}

variable "attachments_base_url" {
  description = "The bucket's public custom-domain HTTPS URL, without a trailing slash."
  type        = string
}

variable "r2_access_key_id" {
  type      = string
  sensitive = true
}

variable "r2_secret_access_key" {
  type      = string
  sensitive = true
}

provider "digitalocean" {
  token = var.digitalocean_token
}

provider "restapi" {
  uri                  = "https://api.digitalocean.com/v2"
  bearer_token         = var.digitalocean_token
  write_returns_object = true
  timeout              = 120
}

resource "digitalocean_vpc" "backend" {
  name   = var.name
  region = "sfo3"
}

resource "digitalocean_database_cluster" "backend" {
  name                 = "${var.name}-pg"
  engine               = "pg"
  version              = "17"
  size                 = "db-s-1vcpu-2gb"
  node_count           = 1
  region               = "sfo3"
  private_network_uuid = digitalocean_vpc.backend.id
  storage_size_mib     = 30720
}

resource "digitalocean_database_db" "backend" {
  cluster_id = digitalocean_database_cluster.backend.id
  name       = "xmtp_backend"
}

resource "digitalocean_database_firewall" "backend" {
  cluster_id = digitalocean_database_cluster.backend.id
  rule {
    type  = "ip_addr"
    value = digitalocean_vpc.backend.ip_range
  }
}

resource "random_password" "api_key" {
  length  = 64
  special = false
}

locals {
  image_reference = trimprefix(var.backend_image, "ghcr.io/xmtp/backend")
  image = merge({
    registry_type = "GHCR"
    registry      = "xmtp"
    repository    = "backend"
    }, startswith(local.image_reference, "@") ? {
    digest = trimprefix(local.image_reference, "@")
    } : {
    tag = trimprefix(local.image_reference, ":")
  })

  database_url = "postgresql://${urlencode(digitalocean_database_cluster.backend.user)}:${urlencode(digitalocean_database_cluster.backend.password)}@${digitalocean_database_cluster.backend.private_host}:${digitalocean_database_cluster.backend.port}/${digitalocean_database_db.backend.name}?sslmode=require"

  # The complete backend config is embedded here; no second config file is needed.
  config = <<-TOML
    [server]
    identifier = ${jsonencode(var.server_identifier)}
    listen = "0.0.0.0:5050"
    log_format = "json"

    [database]
    url = "env:XMTP_DATABASE_URL"
    max_connections = 20

    [auth]
    enabled = true

    [auth.api_keys]
    app = "env:XMTP_BACKEND_API_KEY"

    [attachments]
    base_url = "env:XMTP_ATTACHMENTS_BASE_URL"
    retention_seconds = 0

    [attachments.target.S3]
    endpoint = "env:XMTP_R2_ENDPOINT"
    region = "auto"
    bucket = "env:XMTP_R2_BUCKET"
    key_prefix = ""

    [attachments.target.S3.credentials]
    kind = "environment"
  TOML

  environment = {
    XMTP_CONFIG               = { value = local.config, type = "GENERAL" }
    XMTP_DATABASE_URL         = { value = local.database_url, type = "SECRET" }
    XMTP_BACKEND_API_KEY      = { value = random_password.api_key.result, type = "SECRET" }
    XMTP_ATTACHMENTS_BASE_URL = { value = var.attachments_base_url, type = "GENERAL" }
    XMTP_R2_ENDPOINT          = { value = var.r2_endpoint, type = "SECRET" }
    XMTP_R2_BUCKET            = { value = var.r2_bucket, type = "SECRET" }
    AWS_ACCESS_KEY_ID         = { value = var.r2_access_key_id, type = "SECRET" }
    AWS_SECRET_ACCESS_KEY     = { value = var.r2_secret_access_key, type = "SECRET" }
  }
}

resource "restapi_object" "backend" {
  path         = "/apps"
  id_attribute = "app/id"
  data = jsonencode({
    spec = {
      name   = var.name
      region = "sfo"
      vpc    = { id = digitalocean_vpc.backend.id }
      ingress = {
        rules = [{
          match     = { path = { prefix = "/" } }
          component = { name = "backend", preserve_path_prefix = true }
        }]
      }
      services = [{
        name               = "backend"
        image              = local.image
        http_port          = 5050
        protocol           = "HTTP2"
        instance_count     = 1
        instance_size_slug = "apps-s-1vcpu-2gb"
        # Omit http_path: App Platform's HTTP probes use HTTP/1.1, not h2c.
        health_check = { initial_delay_seconds = 120 }
        termination = {
          drain_seconds        = 15
          grace_period_seconds = 30
        }
        envs = [for key, setting in local.environment : {
          key   = key
          value = setting.value
          type  = setting.type
          scope = "RUN_TIME"
        }]
      }]
    }
  })

  # Preserve our request: DO responses wrap it in "app" and encrypt SECRET values.
  # This also limits drift detection; make app-spec changes through Terraform.
  ignore_server_additions = true
  depends_on              = [digitalocean_database_firewall.backend]

  lifecycle {
    postcondition {
      condition     = jsondecode(self.api_response).app.spec.services[0].protocol == "HTTP2"
      error_message = "The app must retain HTTP2 for native gRPC."
    }
  }
}

output "app_id" {
  value = restapi_object.backend.id
}

output "backend_connection" {
  description = "The URL can be null until the first deployment progresses. Contains a secret."
  sensitive   = true
  value = {
    domain  = try(trimprefix(jsondecode(restapi_object.backend.api_response).app.default_ingress, "https://"), null)
    url     = try(jsondecode(restapi_object.backend.api_response).app.default_ingress, null)
    api_key = random_password.api_key.result
  }
}
