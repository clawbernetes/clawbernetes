import { Rocket, Scale, Pause, PlusCircle, MinusCircle, DollarSign, AlertTriangle, Info } from 'lucide-react';
import clsx from 'clsx';
import type { ActivityItem, ActivityType } from '../types';

interface ActivityFeedProps {
  items: ActivityItem[];
}

const iconMap: Record<ActivityType, { icon: typeof Rocket; color: string }> = {
  deploy: { icon: Rocket, color: 'text-green-400' },
  scale: { icon: Scale, color: 'text-cyan-400' },
  preempt: { icon: Pause, color: 'text-yellow-400' },
  nodeJoin: { icon: PlusCircle, color: 'text-blue-400' },
  nodeLeave: { icon: MinusCircle, color: 'text-red-400' },
  trade: { icon: DollarSign, color: 'text-purple-400' },
  alert: { icon: AlertTriangle, color: 'text-red-400' },
  info: { icon: Info, color: 'text-gray-400' },
};

function formatTime(timestamp: number): string {
  return new Date(timestamp).toLocaleTimeString([], { 
    hour: '2-digit', 
    minute: '2-digit',
    second: '2-digit',
  });
}

export function ActivityFeed({ items }: ActivityFeedProps) {
  return (
    <div className="card h-full flex flex-col">
      <h3 className="card-header">Agent Activity</h3>
      
      <div className="flex-1 overflow-y-auto space-y-0">
        {items.length === 0 ? (
          <p className="text-gray-500 text-sm">No recent activity</p>
        ) : (
          items.slice(0, 20).map((item, i) => {
            const { icon: Icon, color } = iconMap[item.type];
            
            return (
              <div 
                key={`${item.timestamp}-${i}`} 
                className="activity-item animate-fade-in"
              >
                <span className="text-xs text-gray-500 font-mono shrink-0 w-20">
                  {formatTime(item.timestamp)}
                </span>
                <Icon className={clsx('w-4 h-4 shrink-0', color)} />
                <span className="text-sm text-gray-300 truncate">
                  {item.message}
                </span>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
