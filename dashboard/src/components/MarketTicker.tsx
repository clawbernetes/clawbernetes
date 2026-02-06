import { TrendingUp, TrendingDown, Minus } from 'lucide-react';
import clsx from 'clsx';
import type { MarketState } from '../types';

interface MarketTickerProps {
  market: MarketState;
}

export function MarketTicker({ market }: MarketTickerProps) {
  return (
    <div className="card h-full">
      <div className="flex items-center justify-between mb-3">
        <h3 className="card-header mb-0">MOLT Market</h3>
        <div className="text-xs text-gray-500">
          Offers: {market.activeOffers} | Bids: {market.activeBids}
        </div>
      </div>
      
      <div className="space-y-3">
        {market.spotPrices.map((price) => {
          const isUp = price.changePercent > 0;
          const isDown = price.changePercent < 0;
          const TrendIcon = isUp ? TrendingUp : isDown ? TrendingDown : Minus;
          
          return (
            <div key={price.gpuModel} className="flex items-center justify-between">
              <div className="flex items-center gap-3">
                <span className="text-sm font-semibold text-cyan-400 w-12">
                  {price.gpuModel}
                </span>
                <span className="text-lg font-bold text-white">
                  ${price.pricePerHour.toFixed(2)}
                </span>
                <span className="text-xs text-gray-500">/hr</span>
              </div>
              
              <div className="flex items-center gap-3">
                <div className={clsx(
                  'flex items-center gap-1 text-sm',
                  isUp ? 'text-green-400' : isDown ? 'text-red-400' : 'text-gray-400'
                )}>
                  <TrendIcon className="w-3 h-3" />
                  <span>{Math.abs(price.changePercent).toFixed(1)}%</span>
                </div>
                <span className="text-xs text-gray-500 w-16 text-right">
                  {price.availableCount} avail
                </span>
              </div>
            </div>
          );
        })}
      </div>
      
      <div className="mt-4 pt-4 border-t border-gray-800">
        <h4 className="text-xs text-gray-500 uppercase tracking-wide mb-2">Recent Trades</h4>
        <div className="space-y-2">
          {market.recentTrades.slice(0, 3).map((trade, i) => (
            <div key={i} className="flex items-center justify-between text-sm">
              <span className="text-gray-400">
                {trade.gpuCount}x {trade.gpuModel}
              </span>
              <span className="text-purple-400">
                ${trade.pricePerHour.toFixed(2)}/hr × {trade.durationHours}h
              </span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
