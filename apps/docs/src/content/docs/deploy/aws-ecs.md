---
title: AWS ECS (Fargate)
description: Run the XMTP backend on Fargate with a Network Load Balancer and Terraform.
---

Run the published backend image on Fargate. A Network Load Balancer (NLB)
terminates TLS and forwards TCP traffic to the tasks. Read the
[deployment overview](/deploy/overview/) for ports, database connection budgets,
health, shutdown, the ingress contract, and security requirements.

## Prerequisites

Install Terraform from the [HashiCorp installation guide](https://developer.hashicorp.com/terraform/install).
Terraform is not in the repository devshell. This example was checked with
Terraform **1.14.5** and AWS provider **5.100.0**.

Use the AWS console or your infrastructure Terraform project to prepare these
inputs in one AWS Region:

- **VPC and subnets:** Create a VPC with DNS support. Supply two public subnet
  IDs in different Availability Zones for the NLB. Supply private subnet IDs
  for tasks. Private subnets need NAT egress to pull the public image from GHCR
  and reach Secrets Manager and CloudWatch Logs. Tasks have no public IP.
- **Database:** Create an RDS PostgreSQL instance that meets the
  [database requirements](/deploy/overview/#database-and-migrations).
  Use private database subnets in the VPC. After apply, allow database ingress
  from the task security group output on the database port.
- **Database URL secret ARN:** In Secrets Manager, store the complete database
  URL as a plaintext secret value, not a JSON object. Use its secure editor;
  do not put the value in Terraform or a command argument. Copy the secret ARN.
  If you use a customer-managed KMS key, also supply its ARN and allow the
  execution role to use it in the key policy. Otherwise leave that input null.
- **Certificate ARN:** Request an ACM certificate for your DNS name in this
  Region. Complete DNS validation and copy the issued certificate ARN.
- **DNS name:** Use a subdomain in a zone you control, such as
  `xmtp.example.com`. After apply, create a CNAME to the NLB hostname output.
- **Client CIDR:** Supply a trusted client IPv4 CIDR. This minimal configuration
  uses the [restricted-access option](/deploy/overview/#security).

Keep the prerequisite resources in a separate Terraform state if you want
repeatable cleanup. This module does not create or own the RDS instance or secret.

## Configuration

Create a working directory outside the repository. Save this as `config.toml`:

```toml
#:schema https://raw.githubusercontent.com/xmtp/libxmtp/self-hosted/docs/schemas/backend-v1.json
[database]
url = "env:XMTP_DATABASE_URL"
```

The task passes this document through `XMTP_CONFIG`. It uses the published image
directly; no derived image or ECR build is needed. Do not also set `--config-file`.
ECS exposes environment values through `DescribeTaskDefinition`. Use `env:NAME`
references for every sensitive value in the document, never literal secrets.
The database URL arrives separately through ECS `secrets[].valueFrom`.
Never pass a secret as a literal CLI argument: it becomes visible in argv.

## Terraform module

Save this single module as `modules/backend/main.tf` in your working directory.
This module pins AWS provider version 5. The separate security group rule
resources need at least 4.56, and `alpn_policy` needs at least 3.36.

```hcl
terraform {
  required_version = ">= 1.7.0"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
  }
}

variable "region" { type = string }
variable "vpc_id" { type = string }
variable "nlb_subnet_ids" { type = list(string) }
variable "task_subnet_ids" { type = list(string) }
variable "certificate_arn" { type = string }
variable "database_url_secret_arn" { type = string }
variable "dns_name" { type = string }
variable "client_cidr" { type = string }
variable "config_toml" { type = string }
variable "image" { type = string }
variable "secret_kms_key_arn" {
  type    = string
  default = null
}

resource "aws_ecs_cluster" "backend" {
  name = "xmtp"
}

resource "aws_cloudwatch_log_group" "backend" {
  name              = "/ecs/xmtp"
  retention_in_days = 7
}

resource "aws_iam_role" "execution" {
  name_prefix = "xmtp-execution-"
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Principal = { Service = "ecs-tasks.amazonaws.com" }
      Action    = "sts:AssumeRole"
    }]
  })
}

resource "aws_iam_role_policy" "execution" {
  role = aws_iam_role.execution.id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = concat([
      {
        Effect   = "Allow"
        Action   = ["logs:CreateLogStream", "logs:PutLogEvents"]
        Resource = "${aws_cloudwatch_log_group.backend.arn}:*"
      },
      {
        Effect   = "Allow"
        Action   = ["secretsmanager:GetSecretValue"]
        Resource = var.database_url_secret_arn
      }
      ], var.secret_kms_key_arn == null ? [] : [{
        Effect   = "Allow"
        Action   = ["kms:Decrypt"]
        Resource = var.secret_kms_key_arn
    }])
  })
}

resource "aws_security_group" "nlb" {
  name_prefix = "xmtp-nlb-"
  vpc_id      = var.vpc_id
}

resource "aws_security_group" "task" {
  name_prefix = "xmtp-task-"
  vpc_id      = var.vpc_id
}

resource "aws_vpc_security_group_ingress_rule" "tls" {
  security_group_id = aws_security_group.nlb.id
  cidr_ipv4         = var.client_cidr
  ip_protocol       = "tcp"
  from_port         = 443
  to_port           = 443
}

resource "aws_vpc_security_group_egress_rule" "nlb" {
  security_group_id            = aws_security_group.nlb.id
  referenced_security_group_id = aws_security_group.task.id
  ip_protocol                  = "tcp"
  from_port                    = 5050
  to_port                      = 5050
}

resource "aws_vpc_security_group_ingress_rule" "task" {
  security_group_id            = aws_security_group.task.id
  referenced_security_group_id = aws_security_group.nlb.id
  ip_protocol                  = "tcp"
  from_port                    = 5050
  to_port                      = 5050
}

resource "aws_vpc_security_group_egress_rule" "task" {
  security_group_id = aws_security_group.task.id
  cidr_ipv4         = "0.0.0.0/0"
  ip_protocol       = "-1"
}

resource "aws_lb" "backend" {
  name                             = "xmtp"
  internal                         = false
  load_balancer_type               = "network"
  subnets                          = var.nlb_subnet_ids
  security_groups                  = [aws_security_group.nlb.id]
  enable_cross_zone_load_balancing = true
}

resource "aws_lb_target_group" "backend" {
  name                 = "xmtp"
  port                 = 5050
  protocol             = "TCP"
  vpc_id               = var.vpc_id
  target_type          = "ip"
  deregistration_delay = 30
  health_check {
    protocol = "TCP"
  }
}

resource "aws_lb_listener" "tls" {
  load_balancer_arn = aws_lb.backend.arn
  port              = 443
  protocol          = "TLS"
  certificate_arn   = var.certificate_arn
  ssl_policy        = "ELBSecurityPolicy-TLS13-1-2-2021-06"
  alpn_policy       = "HTTP2Preferred"
  default_action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.backend.arn
  }
}

resource "aws_ecs_task_definition" "backend" {
  family                   = "xmtp"
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = "512"
  memory                   = "1024"
  execution_role_arn       = aws_iam_role.execution.arn
  runtime_platform {
    operating_system_family = "LINUX"
    cpu_architecture        = "X86_64"
  }
  container_definitions = jsonencode([{
    name         = "backend"
    image        = var.image
    essential    = true
    stopTimeout  = 60
    portMappings = [{ containerPort = 5050, protocol = "tcp" }]
    environment  = [{ name = "XMTP_CONFIG", value = var.config_toml }]
    secrets = [{
      name      = "XMTP_DATABASE_URL"
      valueFrom = var.database_url_secret_arn
    }]
    healthCheck = {
      command     = ["CMD", "/bin/grpc-health-probe", "-addr=127.0.0.1:5050"]
      interval    = 30
      timeout     = 5
      retries     = 3
      startPeriod = 60
    }
    logConfiguration = {
      logDriver = "awslogs"
      options = {
        awslogs-group         = aws_cloudwatch_log_group.backend.name
        awslogs-region        = var.region
        awslogs-stream-prefix = "backend"
      }
    }
  }])
}

resource "aws_ecs_service" "backend" {
  name                              = "xmtp"
  cluster                           = aws_ecs_cluster.backend.id
  task_definition                   = aws_ecs_task_definition.backend.arn
  desired_count                     = 1
  launch_type                       = "FARGATE"
  health_check_grace_period_seconds = 120
  network_configuration {
    subnets          = var.task_subnet_ids
    security_groups  = [aws_security_group.task.id]
    assign_public_ip = false
  }
  load_balancer {
    target_group_arn = aws_lb_target_group.backend.arn
    container_name   = "backend"
    container_port   = 5050
  }
  depends_on = [aws_lb_listener.tls, aws_iam_role_policy.execution]
}

output "endpoint" {
  value = "https://${var.dns_name}"
}
output "nlb_hostname" {
  value = aws_lb.backend.dns_name
}
output "task_security_group_id" {
  value = aws_security_group.task.id
}
```

The NLB gets its security group at creation. An NLB created without one cannot
have one added later without replacement. The task rule references the NLB
security group and still works if client IP preservation is enabled.

`HTTP2Preferred` supports native gRPC and HTTP/1.1 gRPC-Web. Do not replace this
with an ALB target group set to `protocolVersion: GRPC`: it returns HTTP 464 to
HTTP/1.1 gRPC-Web and CORS preflight requests. The TLS listener also sets the
required `ssl_policy`.

The container probe uses EXEC form because the image has no shell.
`nix/musl-docker.nix` includes the cross-compiled `grpc-health-probe` package in
both image variants, which exposes `/bin/grpc-health-probe`. The NLB check is
only a TCP connect. An HTTP check would send HTTP/1.1 to the h2c port and fail;
NLB target groups have no gRPC matcher. Do not add `path` or `matcher` to this
TCP check.

The NLB TLS listener has a fixed 350 s idle timeout. It cannot be configured.
A TCP listener also defaults to 350 s, but there you can set
`tcp.idle_timeout.seconds` between 60 and 6000. That knob does not apply to a
TLS listener.
The default `streams.keepalive_interval_ms` in `apps/backend/src/config.rs`
uses the shared value `30000` ms (30 s). Server keepalive traffic keeps active
streams inside this limit. The example sets a 30 s deregistration delay and a
60 s container stop timeout. See [shutdown](/deploy/overview/#shutdown) for the
backend drain behavior; the ECS replacement sequence still needs a live check.

## Apply

Save this root invocation as `main.tf`, beside `config.toml`. Replace the example
IDs, ARNs, Region, DNS name, and client CIDR with your prerequisite values.
Select an [immutable image tag](/deploy/overview/#container-image) before production use.

```hcl
terraform {
  required_version = ">= 1.7.0"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
  }
}

provider "aws" {
  region = "us-east-1"
}

module "backend" {
  source                  = "./modules/backend"
  region                  = "us-east-1"
  vpc_id                  = "vpc-0123456789abcdef0"
  nlb_subnet_ids          = ["subnet-0123456789abcdef0", "subnet-0123456789abcdef1"]
  task_subnet_ids         = ["subnet-0123456789abcdef2", "subnet-0123456789abcdef3"]
  certificate_arn         = "arn:aws:acm:us-east-1:123456789012:certificate/00000000-0000-0000-0000-000000000000"
  database_url_secret_arn = "arn:aws:secretsmanager:us-east-1:123456789012:secret:xmtp-database-AbCdEf"
  dns_name                = "xmtp.example.com"
  client_cidr             = "203.0.113.10/32"
  config_toml             = file("${path.module}/config.toml")
  image                   = "ghcr.io/xmtp/backend:self-hosted"
}

output "endpoint" {
  value = module.backend.endpoint
}
output "nlb_hostname" {
  value = module.backend.nlb_hostname
}
output "task_security_group_id" {
  value = module.backend.task_security_group_id
}
```

These local checks need no AWS credentials:

```sh
terraform fmt -check -recursive
terraform init -backend=false
terraform validate
```

With an active AWS session and the prerequisite values in place, deploy:

```sh
terraform plan -out=deployment.tfplan
terraform apply deployment.tfplan
terraform output
```

Allow the task security group through the database security group, then create
the DNS CNAME. Wait for the ECS task and target group to become healthy. Use the
`endpoint` output for clients. Keep the Terraform state and lock file secure.

## Check the deployment and clean up

After `terraform apply`, work through the shared
[ingress checks](/deploy/overview/#ingress-contract):

1. Run `grpc-health-probe` against the NLB hostname with TLS and the certificate
   DNS name as `-tls-server-name`.
2. Use xdbg to generate identities, a group, and messages, then query them back.
3. Use `grpcurl -vv` to inspect response trailers. Check browser gRPC-Web and
   CORS preflight too.
4. Force an ECS task replacement during a subscription. Confirm that the
   deregistration delay permits the backend to drain, rather than cutting the
   subscription. Confirm that the client reconnects and receives later messages.

To tear a test down, run `terraform destroy` here and in the prerequisite
infrastructure project. Confirm that the NLB, RDS instance, and secret are gone.
The NLB and RDS bill hourly; Secrets Manager also charges for retained secrets,
and a secret scheduled for deletion is not yet deleted. Remove the DNS record
too.

Destroying this module alone leaves the database, secret, VPC, and certificate
in place. If you created them in the console, delete them there after the test.
