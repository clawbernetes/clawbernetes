import clsx from 'clsx';
import type { ClusterState } from '../types';

interface WorkloadFlowProps {
  cluster: ClusterState;
}

export function WorkloadFlow({ cluster }: WorkloadFlowProps) {
  const stages = [
    { label: 'Pending', value: cluster.pendingWorkloads, color: 'bg-yellow-500' },
    { label: 'Running', value: cluster.runningWorkloads, color: 'bg-green-500' },
    { label: 'Completed', value: cluster.completedWorkloads, color: 'bg-blue-500' },
  ];
  
  const maxValue = Math.max(...stages.map(s => s.value), 1);
  
  return (
    <div className="card">
      <h3 className="card-header">Workload Flow</h3>
      
      <div className="flex items-center justify-between gap-4">
        {stages.map((stage, i) => (
          <div key={stage.label} className="flex-1 flex items-center">
            <div className="flex-1">
              <div className="text-xs text-gray-500 mb-1">{stage.label}</div>
              <div className="text-2xl font-bold text-white">{stage.value}</div>
              <div className="mt-2 h-2 bg-gray-800 rounded-full overflow-hidden">
                <div 
                  className={clsx(stage.color, 'h-full rounded-full transition-all duration-500')}
                  style={{ width: `${(stage.value / maxValue) * 100}%` }}
                />
              </div>
            </div>
            
            {i < stages.length - 1 && (
              <div className="px-4 text-gray-600">
                <svg className="w-6 h-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
                </svg>
              </div>
            )}
          </div>
        ))}
      </div>
      
      {cluster.failedWorkloads > 0 && (
        <div className="mt-4 pt-4 border-t border-gray-800 flex items-center gap-2 text-red-400">
          <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
          </svg>
          <span className="text-sm">{cluster.failedWorkloads} failed workloads</span>
        </div>
      )}
    </div>
  );
}
