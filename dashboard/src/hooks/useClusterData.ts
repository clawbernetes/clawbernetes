import { useState, useEffect, useCallback } from 'react';
import type { DashboardState, ActivityItem, ActivityType } from '../types';
import { generateDemoData } from '../utils/demoData';

const POLL_INTERVAL = 2000;

interface UseClusterDataOptions {
  demo?: boolean;
  bridgeUrl?: string;
}

export function useClusterData(options: UseClusterDataOptions = {}) {
  const { demo = true } = options;
  
  const [state, setState] = useState<DashboardState>({
    cluster: {
      totalNodes: 0,
      healthyNodes: 0,
      totalGpus: 0,
      availableGpus: 0,
      totalMemoryGb: 0,
      usedMemoryGb: 0,
      runningWorkloads: 0,
      pendingWorkloads: 0,
      completedWorkloads: 0,
      failedWorkloads: 0,
    },
    nodes: [],
    workloads: [],
    activity: [],
    market: {
      spotPrices: [],
      activeOffers: 0,
      activeBids: 0,
      recentTrades: [],
    },
    connected: false,
    lastUpdate: undefined,
  });

  const addActivity = useCallback((type: ActivityType, message: string, details?: string) => {
    const item: ActivityItem = {
      timestamp: Date.now(),
      type,
      message,
      details,
    };
    setState(prev => ({
      ...prev,
      activity: [item, ...prev.activity.slice(0, 99)],
    }));
  }, []);

  useEffect(() => {
    if (!demo) {
      // Real mode - would connect to WebSocket or poll API
      // For now, just mark as disconnected
      setState(prev => ({ ...prev, connected: false }));
      return;
    }

    // Demo mode - generate fake data periodically
    const updateDemoData = () => {
      const demoData = generateDemoData();
      setState(prev => ({
        ...prev,
        ...demoData,
        connected: true,
        lastUpdate: Date.now(),
      }));
    };

    // Initial load
    updateDemoData();

    // Periodic updates
    const interval = setInterval(updateDemoData, POLL_INTERVAL);

    // Random activity events
    const activityInterval = setInterval(() => {
      const events: [ActivityType, string][] = [
        ['deploy', 'Deployed pytorch/pytorch:2.1-cuda12'],
        ['scale', 'Scaling pool-prod: 3→5 nodes'],
        ['trade', 'MOLT bid accepted: 4x H100 @ $2.41/hr'],
        ['nodeJoin', 'Node gpu-node-6 joined cluster'],
        ['preempt', 'Preempting low-priority job j-abc123'],
        ['alert', 'GPU temp >85°C on gpu-node-2'],
        ['info', 'Autoscaler evaluation complete'],
      ];
      const [type, message] = events[Math.floor(Math.random() * events.length)];
      addActivity(type, message);
    }, 5000);

    return () => {
      clearInterval(interval);
      clearInterval(activityInterval);
    };
  }, [demo, addActivity]);

  return { state, addActivity };
}
