# Clawbernetes Fleet Management Skill

An OpenClaw skill for AI-assisted management of Clawbernetes GPU compute clusters.

## Overview

This skill enables Claude (or other AI assistants using OpenClaw) to help manage Clawbernetes clusters, including:

- **Cluster Monitoring** - Check node status, GPU availability, and cluster health
- **Workload Management** - Submit, monitor, and manage GPU workloads
- **MOLT Network** - Participate in GPU sharing for earnings

## Installation

Copy this skill directory to your OpenClaw skills folder:

```bash
cp -r clawbernetes ~/.openclaw/skills/
```

Or symlink for development:

```bash
ln -s $(pwd) ~/.openclaw/skills/clawbernetes
```

## Files

```
clawbernetes/
├── SKILL.md              # Main skill instructions (loaded by AI)
├── README.md             # This file
├── scripts/
│   └── status.sh         # Pretty-print cluster status
└── templates/
    └── workload.yaml     # Example workload configuration
```

## Usage

Once installed, the AI will automatically use this skill when you mention:
- GPU clusters or nodes
- Clawbernetes commands
- Workload submission or monitoring
- MOLT network participation

### Example Conversations

**Checking cluster status:**
> "What's the status of my GPU cluster?"

**Submitting a job:**
> "Run a PyTorch training job with 4 A100s"

**Monitoring workloads:**
> "Check on my training job and show me the logs"

**MOLT participation:**
> "How much have I earned from MOLT this week?"

## Helper Scripts

### status.sh

Pretty-prints cluster status with colors and formatting:

```bash
# Direct usage
clawbernetes status --json | ./scripts/status.sh

# Watch mode (updates every 5 seconds)
./scripts/status.sh --watch

# Custom interval
WATCH_INTERVAL=10 ./scripts/status.sh --watch
```

## Templates

### workload.yaml

A comprehensive workload template with all available options documented. Use as a starting point:

```bash
# Copy and customize
cp templates/workload.yaml my-job.yaml
vim my-job.yaml

# Submit
clawbernetes run --file my-job.yaml
```

## Configuration

### Gateway URL

Set the Clawbernetes gateway URL:

```bash
export CLAWBERNETES_GATEWAY_URL="ws://your-gateway:9000"
```

### Authentication

Authenticate with your cluster:

```bash
clawbernetes auth login
```

## Requirements

- Clawbernetes CLI installed (`clawbernetes` command available)
- `jq` for status formatting script
- Network access to Clawbernetes gateway

## Development

To modify this skill:

1. Edit `SKILL.md` to change AI instructions
2. Add helper scripts to `scripts/`
3. Add templates to `templates/`
4. Test with OpenClaw

## License

MIT License - Part of the Clawbernetes project.
