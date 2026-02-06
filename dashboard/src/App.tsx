import { Header, StatsCards, GpuHeatmap, ActivityFeed, MarketTicker, WorkloadFlow } from './components';
import { useClusterData } from './hooks/useClusterData';

function App() {
  const { state } = useClusterData({ demo: true });

  return (
    <div className="min-h-screen bg-claw-darker grid-bg">
      <Header connected={state.connected} lastUpdate={state.lastUpdate} />
      
      <main className="p-6 max-w-[1800px] mx-auto">
        {/* Stats Row */}
        <div className="mb-6">
          <StatsCards cluster={state.cluster} />
        </div>
        
        {/* Main Grid */}
        <div className="grid grid-cols-12 gap-6">
          {/* Left Column */}
          <div className="col-span-8 space-y-6">
            {/* GPU Heatmap */}
            <GpuHeatmap nodes={state.nodes} />
            
            {/* Workload Flow */}
            <WorkloadFlow cluster={state.cluster} />
          </div>
          
          {/* Right Column */}
          <div className="col-span-4 space-y-6">
            {/* Activity Feed */}
            <div className="h-80">
              <ActivityFeed items={state.activity} />
            </div>
            
            {/* Market Ticker */}
            <MarketTicker market={state.market} />
          </div>
        </div>
        
        {/* Footer */}
        <footer className="mt-6 text-center text-xs text-gray-600">
          <p>CLAWBERNETES Control Room • Read-Only View • AI-Native GPU Orchestration</p>
        </footer>
      </main>
    </div>
  );
}

export default App;
