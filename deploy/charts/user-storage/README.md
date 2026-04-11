# User Storage Helm Chart

## Overview

This Helm chart deploys the `user-storage` application on a Kubernetes cluster. It includes configurations for the application and its dependencies.

## Prerequisites

Before installing this chart, ensure you have the following tools installed:

- [kubectl](https://kubernetes.io/docs/tasks/tools/): Kubernetes command-line tool.
- [Helm](https://helm.sh/docs/intro/install/): Kubernetes package manager.
- A running Kubernetes cluster.

## Dependencies

This chart requires the following services to be running:

1. **PostgreSQL**
2. **Redis**
3. **Keycloak**
4. **MinIO**

### Quick Start: Kubernetes Dependencies

For local development (Kind, Minikube, etc.), you can quickly spin up all dependencies using the provided manifests:

```bash
kubectl apply -f ../../k8s/dependencies/
```

This will deploy:

- **PostgreSQL**: `postgres:5432` (User/DB: `postgres`, Pass: `postgres`)
- **Redis**: `redis:6379`
- **MinIO**: `minio:9000` (Console: `9001`, Credentials: `minioadmin` / `minioadmin`, bucket `user-storage-dev` auto-created)
- **Keycloak**: `keycloak:9026` (Credentials: `admin` / `admin`)

### Alternative: Docker Compose Dependencies

#### PostgreSQL

To deploy PostgreSQL, use the provided `postgres.compose.yml` file:

```bash
cd deploy/compose
export POSTGRES_PORT=5432
export POSTGRES_USER=postgres
export POSTGRES_PASSWORD=postgres
export POSTGRES_DB=user-storage
docker-compose -f postgres.compose.yml up -d
```

#### Redis

To deploy Redis, use the `redis.compose.yml` file:

```bash
cd deploy/compose
docker-compose -f redis.compose.yml up -d
```

#### Keycloak

To deploy Keycloak, use the `keycloak.compose.yml` file:

```bash
cd deploy/compose
docker-compose -f keycloak.compose.yml up -d
```

#### MinIO

To deploy MinIO, use the `minio.compose.yml` file:

```bash
cd deploy/compose
docker-compose -f minio.compose.yml up -d
```

## Installing the Helm Chart

1. Add the Helm repository for the `common` dependency:

```bash
helm repo add bjw-s https://bjw-s-labs.github.io/helm-charts
helm repo update
```

2. Install the `user-storage` chart:

```bash
cd deploy/charts/user-storage
helm dependency update
helm install user-storage .
```

### Quick Dev Setup

For a quick development setup that points to the local Kubernetes dependencies (see [Quick Start: Kubernetes Dependencies](#quick-start-kubernetes-dependencies)), use the provided `values-dev.yaml`:

```bash
cd deploy/charts/user-storage
helm install user-storage . -f values-dev.yaml
```

## Configuration

The chart values can be customized by editing the `values.yaml` file. For example:

```yaml
replicaCount: 2
image:
  repository: my-repo/user-storage
  tag: latest
  pullPolicy: IfNotPresent
```

To apply custom values, use the `--set` flag or provide a custom `values.yaml` file:

```bash
helm install user-storage . -f custom-values.yaml
```

## Uninstalling the Chart

To uninstall the `user-storage` release:

```bash
helm uninstall user-storage
```

## Namespace

You may need to create the namespaces manually if they don't exist

```bash
kubectl create namespace user-storage-deps
kubectl create namespace user-storage
```
