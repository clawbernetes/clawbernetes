// Cluster Types

export interface ClusterState {
  totalNodes: number;
  healthyNodes: number;
  totalGpus: number;
  availableGpus: number;
  totalMemoryGb: number;
  usedMemoryGb: number;
  runningWorkloads: number;
  pendingWorkloads: number;
  completedWorkloads: number;
  failedWorkloads: number;
}

export type NodeStatus = 'healthy' | 'unhealthy' | 'draining' | 'offline';

export interface NodeState {
  id: string;
  name: string;
  status: NodeStatus;
  gpus: GpuState[];
  cpuPercent: number;
  memoryUsedMb: number;
  memoryTotalMb: number;
  workloadCount: number;
  lastHeartbeat?: number;
}

export interface GpuState {
  index: number;
  name: string;
  utilizationPercent: number;
  memoryUsedMb: number;
  memoryTotalMb: number;
  temperatureC: number;
  powerDrawW?: number;
}

export type WorkloadStatus = 'pending' | 'scheduling' | 'running' | 'completed' | 'failed';

export interface WorkloadState {
  id: string;
  name?: string;
  image: string;
  state: WorkloadStatus;
  gpuCount: number;
  assignedNode?: string;
  startedAt?: number;
  progressPercent?: number;
}

export type ActivityType = 'scale' | 'deploy' | 'preempt' | 'nodeJoin' | 'nodeLeave' | 'trade' | 'alert' | 'info';

export interface ActivityItem {
  timestamp: number;
  type: ActivityType;
  message: string;
  details?: string;
}

export interface SpotPrice {
  gpuModel: string;
  pricePerHour: number;
  changePercent: number;
  availableCount: number;
}

export interface Trade {
  timestamp: number;
  gpuModel: string;
  gpuCount: number;
  pricePerHour: number;
  durationHours: number;
}

export interface MarketState {
  spotPrices: SpotPrice[];
  activeOffers: number;
  activeBids: number;
  recentTrades: Trade[];
}

// Dashboard state
export interface DashboardState {
  cluster: ClusterState;
  nodes: NodeState[];
  workloads: WorkloadState[];
  activity: ActivityItem[];
  market: MarketState;
  connected: boolean;
  lastUpdate?: number;
}
