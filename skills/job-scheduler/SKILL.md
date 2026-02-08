# Job Scheduler

You can create and manage batch jobs and cron schedules on Clawbernetes nodes.

## Job Commands

Jobs run containers to completion. Unlike workloads (which run indefinitely), jobs track completions, failures, and support parallelism.

### Create a Job

```
node.invoke <node-id> job.create {
  "name": "data-export",
  "image": "myapp:latest",
  "command": ["python", "export.py"],
  "completions": 1,
  "parallelism": 1,
  "backoffLimit": 3
}
```

**Parameters:**
- `name` (required): Job name
- `image` (required): Container image to run
- `command` (optional): Command to execute
- `completions` (optional, default 1): Number of successful completions required
- `parallelism` (optional, default 1): Maximum concurrent pods
- `backoffLimit` (optional, default 3): Maximum retries on failure

**Returns:** `name`, `state`, `success`

### Check Job Status

```
node.invoke <node-id> job.status { "name": "data-export" }
```

Returns: name, state (running/completed/failed), completed/failed counts, duration, container IDs.

### Get Job Logs

```
node.invoke <node-id> job.logs { "name": "data-export", "tail": 100 }
```

Returns aggregated logs from all job containers.

### Delete a Job

```
node.invoke <node-id> job.delete { "name": "data-export" }
```

Stops running containers and removes the job entry.

## Cron Commands

Cron jobs run on a schedule, creating job instances at specified intervals.

### Create a Cron Job

```
node.invoke <node-id> cron.create {
  "name": "nightly-backup",
  "schedule": "0 2 * * *",
  "image": "backup-tool:latest",
  "command": ["backup.sh", "--full"]
}
```

**Parameters:**
- `name` (required): Cron job name
- `schedule` (required): 5-field cron expression (minute hour day-of-month month day-of-week)
- `image` (required): Container image
- `command` (optional): Command to execute

**Schedule examples:**
- `0 2 * * *` — Every day at 2:00 AM
- `*/15 * * * *` — Every 15 minutes
- `0 0 * * 0` — Every Sunday at midnight
- `0 6,18 * * 1-5` — 6 AM and 6 PM on weekdays

### List Cron Jobs

```
node.invoke <node-id> cron.list
```

Returns all cron jobs with schedule, last run, next run, and suspended status.

### Trigger a Cron Job Immediately

```
node.invoke <node-id> cron.trigger { "name": "nightly-backup" }
```

Creates a job instance immediately, regardless of the schedule.

### Suspend a Cron Job

```
node.invoke <node-id> cron.suspend { "name": "nightly-backup" }
```

### Resume a Cron Job

```
node.invoke <node-id> cron.resume { "name": "nightly-backup" }
```

## Common Patterns

### GPU Training Job
```json
{
  "name": "train-model-v3",
  "image": "pytorch/pytorch:2.1.0-cuda12.1-cudnn8-runtime",
  "command": ["python", "train.py", "--epochs", "50"],
  "completions": 1,
  "backoffLimit": 2
}
```

### Data Pipeline (Parallel)
```json
{
  "name": "process-shards",
  "image": "etl-pipeline:latest",
  "command": ["process.py"],
  "completions": 10,
  "parallelism": 5
}
```

### Scheduled Cleanup
```json
{
  "name": "cleanup-old-checkpoints",
  "schedule": "0 3 * * *",
  "image": "cleanup:latest",
  "command": ["cleanup.sh", "--older-than", "7d"]
}
```

## Workflow

1. `job.create` to run a batch task
2. `job.status` to monitor progress
3. `job.logs` to check output
4. For recurring tasks, use `cron.create` with a schedule
5. `cron.suspend` / `cron.resume` to pause and restart schedules
