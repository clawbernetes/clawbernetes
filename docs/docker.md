# Docker Deployment

Run Clawbernetes in containers for quick testing or production deployment.

---

## Quick Start

```bash
make docker-up      # gateway + 2 simulated nodes
make docker-logs    # follow logs
make docker-down    # tear down
```

---

## GPU Nodes

For real GPU passthrough, uncomment the `gpu-node` service in `docker-compose.yml`. Requires [nvidia-container-toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/install-guide.html).

```bash
make docker-gpu     # build GPU node image
```

---

## Building Images

```bash
docker build --target gateway -t clawbernetes/gateway:latest .
docker build --target node -t clawbernetes/node:latest .
docker build -f Dockerfile.gpu -t clawbernetes/node-gpu:latest .
```

---

## See Also

- [Configuration](configuration.md) — Gateway, node, and plugin config
- [Development](development.md) — Building from source
