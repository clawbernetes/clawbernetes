import { Server, Cpu, Play, CheckCircle } from 'lucide-react';
import type { ClusterState } from '../types';

interface StatsCardsProps {
  cluster: ClusterState;
}

export function StatsCards({ cluster }: StatsCardsProps) {
  const stats = [
    {
      label: 'Nodes',
      value: `${cluster.healthyNodes}/${cluster.totalNodes}`,
      detail: 'healthy',
      icon: Server,
      color: cluster.healthyNodes === cluster.totalNodes ? 'text-green-400' : 'text-yellow-400',
    },
    {
      label: 'GPUs',
      value: `${cluster.availableGpus}/${cluster.totalGpus}`,
      detail: 'available',
      icon: Cpu,
      color: 'text-blue-400',
    },
    {
      label: 'Running',
      value: cluster.runningWorkloads.toString(),
      detail: 'workloads',
      icon: Play,
      color: 'text-claw-accent',
    },
    {
      label: 'Completed',
      value: cluster.completedWorkloads.toString(),
      detail: 'total',
      icon: CheckCircle,
      color: 'text-purple-400',
    },
  ];

  return (
    <div className="grid grid-cols-4 gap-4">
      {stats.map((stat) => (
        <div key={stat.label} className="card">
          <div className="flex items-start justify-between">
            <div>
              <p className="stat-label">{stat.label}</p>
              <p className={`stat-value ${stat.color}`}>{stat.value}</p>
              <p className="text-xs text-gray-500 mt-1">{stat.detail}</p>
            </div>
            <stat.icon className={`w-8 h-8 ${stat.color} opacity-50`} />
          </div>
        </div>
      ))}
    </div>
  );
}
