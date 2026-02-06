import clsx from 'clsx';
import type { NodeState } from '../types';

interface GpuHeatmapProps {
  nodes: NodeState[];
}

function getUtilColor(percent: number): string {
  if (percent >= 90) return 'bg-red-500';
  if (percent >= 70) return 'bg-yellow-500';
  if (percent >= 40) return 'bg-green-500';
  return 'bg-blue-500';
}

function getStatusColor(status: NodeState['status']): string {
  switch (status) {
    case 'healthy': return 'text-green-400';
    case 'unhealthy': return 'text-red-400';
    case 'draining': return 'text-yellow-400';
    case 'offline': return 'text-gray-500';
  }
}

export function GpuHeatmap({ nodes }: GpuHeatmapProps) {
  return (
    <div className="card h-full">
      <h3 className="card-header">GPU Heatmap</h3>
      
      <div className="space-y-3">
        {nodes.length === 0 ? (
          <p className="text-gray-500 text-sm">No nodes connected</p>
        ) : (
          nodes.map((node) => {
            const avgUtil = node.gpus.length > 0
              ? Math.round(node.gpus.reduce((sum, g) => sum + g.utilizationPercent, 0) / node.gpus.length)
              : 0;
            
            return (
              <div key={node.id} className="flex items-center gap-3">
                <div className={clsx('w-24 truncate text-sm font-mono', getStatusColor(node.status))}>
                  {node.name}
                </div>
                
                <div className="flex gap-1 flex-1">
                  {node.gpus.map((gpu) => (
                    <div
                      key={gpu.index}
                      className={clsx(
                        'gpu-block',
                        getUtilColor(gpu.utilizationPercent),
                        'opacity-80 hover:opacity-100'
                      )}
                      style={{
                        opacity: 0.3 + (gpu.utilizationPercent / 100) * 0.7,
                      }}
                      title={`GPU ${gpu.index}: ${gpu.utilizationPercent}% | ${gpu.temperatureC}°C | ${Math.round(gpu.memoryUsedMb / 1024)}/${Math.round(gpu.memoryTotalMb / 1024)}GB`}
                    />
                  ))}
                </div>
                
                <div className={clsx(
                  'text-sm font-mono w-12 text-right',
                  avgUtil >= 80 ? 'text-red-400' : avgUtil >= 50 ? 'text-yellow-400' : 'text-green-400'
                )}>
                  {avgUtil}%
                </div>
              </div>
            );
          })
        )}
      </div>
      
      <div className="mt-4 flex items-center gap-4 text-xs text-gray-500">
        <span>Utilization:</span>
        <div className="flex items-center gap-2">
          <div className="w-3 h-3 rounded bg-blue-500" /> Low
          <div className="w-3 h-3 rounded bg-green-500" /> Med
          <div className="w-3 h-3 rounded bg-yellow-500" /> High
          <div className="w-3 h-3 rounded bg-red-500" /> Max
        </div>
      </div>
    </div>
  );
}
