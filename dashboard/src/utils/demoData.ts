import type { DashboardState, NodeState, GpuState, WorkloadState, SpotPrice, Trade } from '../types';

const nodeConfigs = [
  { id: 'gpu-node-1', name: 'gpu-node-1', gpuModel: 'H100', gpuCount: 4 },
  { id: 'gpu-node-2', name: 'gpu-node-2', gpuModel: 'H100', gpuCount: 4 },
  { id: 'gpu-node-3', name: 'gpu-node-3', gpuModel: 'A100', gpuCount: 8 },
  { id: 'gpu-node-4', name: 'gpu-node-4', gpuModel: 'A100', gpuCount: 8 },
  { id: 'gpu-node-5', name: 'gpu-node-5', gpuModel: 'L40S', gpuCount: 4 },
];

const workloadImages = [
  'pytorch/pytorch:2.1-cuda12',
  'nvcr.io/nvidia/tensorflow:23.12',
  'huggingface/transformers-pytorch-gpu',
  'stable-diffusion:xl',
  'llama:70b-chat',
];

function rand(min: number, max: number): number {
  return Math.floor(Math.random() * (max - min + 1)) + min;
}

function randFloat(min: number, max: number): number {
  return Math.random() * (max - min) + min;
}

function generateGpus(model: string, count: number): GpuState[] {
  return Array.from({ length: count }, (_, i) => ({
    index: i,
    name: model,
    utilizationPercent: rand(0, 100),
    memoryUsedMb: rand(10000, 80000),
    memoryTotalMb: 80000,
    temperatureC: rand(45, 85),
    powerDrawW: randFloat(200, 400),
  }));
}

function generateNodes(): NodeState[] {
  return nodeConfigs.map(config => {
    const gpus = generateGpus(config.gpuModel, config.gpuCount);
    return {
      id: config.id,
      name: config.name,
      status: Math.random() > 0.9 ? 'unhealthy' : 'healthy',
      gpus,
      cpuPercent: randFloat(10, 90),
      memoryUsedMb: rand(32000, 128000),
      memoryTotalMb: 256000,
      workloadCount: rand(1, 5),
      lastHeartbeat: Date.now() - rand(0, 30000),
    };
  });
}

function generateWorkloads(): WorkloadState[] {
  const count = rand(8, 20);
  return Array.from({ length: count }, (_, i) => {
    const states: WorkloadState['state'][] = ['pending', 'running', 'running', 'running', 'completed'];
    return {
      id: `wl-${Math.random().toString(36).substring(2, 10)}`,
      name: `training-job-${i + 1}`,
      image: workloadImages[rand(0, workloadImages.length - 1)],
      state: states[rand(0, states.length - 1)],
      gpuCount: rand(1, 8),
      assignedNode: nodeConfigs[rand(0, nodeConfigs.length - 1)].id,
      startedAt: Date.now() - rand(0, 3600000),
      progressPercent: rand(0, 100),
    };
  });
}

function generateSpotPrices(): SpotPrice[] {
  return [
    { gpuModel: 'H100', pricePerHour: 2.41 + randFloat(-0.2, 0.2), changePercent: randFloat(-5, 5), availableCount: rand(10, 50) },
    { gpuModel: 'A100', pricePerHour: 1.85 + randFloat(-0.1, 0.1), changePercent: randFloat(-3, 3), availableCount: rand(20, 100) },
    { gpuModel: 'L40S', pricePerHour: 0.92 + randFloat(-0.05, 0.05), changePercent: randFloat(-2, 2), availableCount: rand(50, 200) },
    { gpuModel: '4090', pricePerHour: 0.45 + randFloat(-0.02, 0.02), changePercent: randFloat(-1, 1), availableCount: rand(100, 500) },
  ];
}

function generateTrades(): Trade[] {
  return Array.from({ length: rand(3, 8) }, () => ({
    timestamp: Date.now() - rand(0, 3600000),
    gpuModel: ['H100', 'A100', 'L40S', '4090'][rand(0, 3)],
    gpuCount: rand(1, 8),
    pricePerHour: randFloat(0.5, 2.5),
    durationHours: rand(1, 24),
  }));
}

export function generateDemoData(): Partial<DashboardState> {
  const nodes = generateNodes();
  const workloads = generateWorkloads();
  
  const totalGpus = nodes.reduce((sum, n) => sum + n.gpus.length, 0);
  const healthyNodes = nodes.filter(n => n.status === 'healthy').length;
  const runningWorkloads = workloads.filter(w => w.state === 'running').length;
  const pendingWorkloads = workloads.filter(w => w.state === 'pending').length;
  const completedWorkloads = rand(100, 200);
  const failedWorkloads = rand(0, 5);
  
  const avgUtil = nodes.flatMap(n => n.gpus).reduce((sum, g) => sum + g.utilizationPercent, 0) / totalGpus;
  const availableGpus = Math.floor(totalGpus * (100 - avgUtil) / 100);
  
  return {
    cluster: {
      totalNodes: nodes.length,
      healthyNodes,
      totalGpus,
      availableGpus,
      totalMemoryGb: 560,
      usedMemoryGb: rand(200, 400),
      runningWorkloads,
      pendingWorkloads,
      completedWorkloads,
      failedWorkloads,
    },
    nodes,
    workloads,
    market: {
      spotPrices: generateSpotPrices(),
      activeOffers: rand(30, 80),
      activeBids: rand(10, 40),
      recentTrades: generateTrades(),
    },
  };
}
