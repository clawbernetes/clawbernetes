import { Zap } from 'lucide-react';
import clsx from 'clsx';

interface HeaderProps {
  connected: boolean;
  lastUpdate?: number;
}

export function Header({ connected, lastUpdate }: HeaderProps) {
  const formatTime = (ts?: number) => {
    if (!ts) return '--:--:--';
    return new Date(ts).toLocaleTimeString();
  };

  return (
    <header className="bg-claw-dark border-b border-gray-800 px-6 py-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-4">
          <div className="flex items-center gap-2">
            <Zap className="w-6 h-6 text-claw-accent" />
            <h1 className="text-xl font-bold text-white tracking-tight">
              CLAWBERNETES
            </h1>
          </div>
          <span className="text-xs text-gray-500 uppercase tracking-widest">
            Control Room
          </span>
        </div>
        
        <div className="flex items-center gap-6">
          <div className="flex items-center gap-2 text-sm">
            <span className="text-gray-500">Last update:</span>
            <span className="text-gray-400 font-mono">{formatTime(lastUpdate)}</span>
          </div>
          
          <div className="flex items-center gap-2">
            <div className={clsx(
              'w-2 h-2 rounded-full',
              connected ? 'bg-green-400 animate-pulse' : 'bg-red-400'
            )} />
            <span className={clsx(
              'text-sm',
              connected ? 'text-green-400' : 'text-red-400'
            )}>
              {connected ? 'Connected' : 'Disconnected'}
            </span>
          </div>
        </div>
      </div>
    </header>
  );
}
