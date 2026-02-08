---
name: storage
description: Volume provisioning, snapshots, and backup management for Clawbernetes nodes
metadata:
  openclaw:
    requires:
      bins: ["clawnode"]
---

# Storage & Volumes

You can provision, mount, snapshot, and manage persistent volumes and backups on Clawbernetes nodes.

Requires the `storage` feature on the node.

## Volume Commands

### Create a Volume

```
node.invoke <node-id> volume.create {
  "name": "training-data",
  "type": "emptydir",
  "sizeGb": 100,
  "accessMode": "ReadWriteOnce"
}
```

**Parameters:**
- `name` (required): Volume name
- `type` (optional, default "emptydir"): Volume type — `emptydir`, `hostpath`, `nfs`, `s3`
- `sizeGb` (optional, default 10): Volume size in GB
- `accessMode` (optional, default "ReadWriteOnce"): `ReadWriteOnce`, `ReadOnlyMany`, `ReadWriteMany`
- `storageClass` (optional): Storage class name
- `server` (required for nfs): NFS server address
- `path` (required for nfs, optional for hostpath): Mount path
- `bucket` (required for s3): S3 bucket name

### Mount a Volume

```
node.invoke <node-id> volume.mount {
  "volumeId": "training-data",
  "workloadId": "train-job-abc",
  "mountPath": "/data",
  "readOnly": false
}
```

### Unmount a Volume

```
node.invoke <node-id> volume.unmount { "volumeId": "training-data" }
```

### Create a Snapshot

```
node.invoke <node-id> volume.snapshot { "volumeId": "training-data" }
```

Returns a snapshot ID and timestamp.

### List Volumes

```
node.invoke <node-id> volume.list
```

Returns total, available, bound, attached volume counts and total capacity.

### Delete a Volume

```
node.invoke <node-id> volume.delete { "volumeId": "training-data" }
```

## Backup Commands

### Create a Backup

```
node.invoke <node-id> backup.create {
  "scope": "full",
  "destination": "s3://backups/daily"
}
```

**Parameters:**
- `scope` (required): What to back up (e.g., "full", "volumes", "configs")
- `destination` (required): Backup destination path

### Restore from Backup

```
node.invoke <node-id> backup.restore { "backupId": "<backup-id>" }
```

### List Backups

```
node.invoke <node-id> backup.list
```

## Volume Types

| Type | Use Case | Extra Params |
|------|----------|-------------|
| `emptydir` | Temporary scratch space | None |
| `hostpath` | Access host filesystem | `path` |
| `nfs` | Shared network storage | `server`, `path` |
| `s3` | Object storage | `bucket` |

## Common Patterns

### Training Data Volume
```json
{
  "name": "datasets",
  "type": "nfs",
  "sizeGb": 500,
  "accessMode": "ReadOnlyMany",
  "server": "nfs.internal",
  "path": "/exports/datasets"
}
```

### Model Checkpoint Storage
```json
{
  "name": "checkpoints",
  "type": "hostpath",
  "sizeGb": 200,
  "path": "/data/checkpoints"
}
```

### Daily Backup Schedule
Use with cron jobs:
```
cron.create {"name": "daily-backup", "schedule": "0 2 * * *", "image": "backup:latest", "command": ["backup.sh"]}
```
Then the backup job calls `backup.create` internally.
