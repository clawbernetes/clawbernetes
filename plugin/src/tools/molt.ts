/**
 * MOLT Marketplace Tools
 *
 * Tools for the P2P GPU marketplace - listing offers, bidding, and capacity management.
 */

import { Type } from "@sinclair/typebox";
import type { ClawbernnetesTool, ToolResult } from "../types.js";
import { getClient } from "../client.js";

function jsonResult(data: unknown): ToolResult {
  return { type: "json", content: JSON.stringify(data, null, 2) };
}

function errorResult(message: string): ToolResult {
  return { type: "error", content: message };
}

// ─────────────────────────────────────────────────────────────
// molt_offers
// ─────────────────────────────────────────────────────────────

const MoltOffersSchema = Type.Object({
  minGpus: Type.Optional(Type.Number({ description: "Minimum GPUs required" })),
  maxPricePerHour: Type.Optional(Type.Number({ description: "Maximum price per hour (USD)" })),
  region: Type.Optional(Type.String({ description: "Region filter (e.g., us-west, eu-central)" })),
  gpuModel: Type.Optional(Type.String({ description: "GPU model filter (e.g., H100, A100)" })),
});

export function createMoltOffersTool(): ClawbernnetesTool {
  return {
    name: "molt_offers",
    label: "MOLT",
    description:
      "List available GPU capacity offers on the MOLT P2P marketplace. Filter by GPUs, price, region, or model.",
    parameters: MoltOffersSchema,
    execute: async (_id, args, context) => {
      try {
        const client = getClient(context?.config);
        const params = args as {
          minGpus?: number;
          maxPricePerHour?: number;
          region?: string;
          gpuModel?: string;
        };

        const offers = await client.listMoltOffers({
          minGpus: params.minGpus,
          maxPricePerHour: params.maxPricePerHour,
          region: params.region,
          gpuModel: params.gpuModel,
        });

        return jsonResult({
          count: offers.length,
          offers: offers.map((o) => ({
            id: o.id,
            gpus: o.gpus,
            gpuModel: o.gpuModel,
            pricePerHour: `$${o.pricePerHour.toFixed(2)}/hr`,
            region: o.region,
            minDuration: o.minDurationHours ? `${o.minDurationHours}h` : null,
            maxDuration: o.maxDurationHours ? `${o.maxDurationHours}h` : null,
            availableAt: new Date(o.availableAt).toISOString(),
          })),
          summary: offers.length > 0
            ? {
                cheapest: Math.min(...offers.map((o) => o.pricePerHour)),
                totalGpus: offers.reduce((a, b) => a + b.gpus, 0),
                regions: [...new Set(offers.map((o) => o.region))],
              }
            : null,
        });
      } catch (err) {
        return errorResult(`Failed to list MOLT offers: ${err}`);
      }
    },
  };
}

// ─────────────────────────────────────────────────────────────
// molt_offer_create
// ─────────────────────────────────────────────────────────────

const MoltOfferCreateSchema = Type.Object({
  gpus: Type.Number({ description: "Number of GPUs to offer", minimum: 1 }),
  gpuModel: Type.String({ description: "GPU model (e.g., H100, A100, RTX4090)" }),
  pricePerHour: Type.Number({ description: "Price per hour in USD", minimum: 0.01 }),
  minDurationHours: Type.Optional(Type.Number({ description: "Minimum rental duration (hours)" })),
  maxDurationHours: Type.Optional(Type.Number({ description: "Maximum rental duration (hours)" })),
});

export function createMoltOfferCreateTool(): ClawbernnetesTool {
  return {
    name: "molt_offer_create",
    label: "MOLT",
    description:
      "Create a new offer to sell GPU capacity on the MOLT marketplace. Your GPUs will be available for rental.",
    parameters: MoltOfferCreateSchema,
    execute: async (_id, args, context) => {
      try {
        const client = getClient(context?.config);
        const params = args as {
          gpus: number;
          gpuModel: string;
          pricePerHour: number;
          minDurationHours?: number;
          maxDurationHours?: number;
        };

        const offer = await client.createMoltOffer({
          gpus: params.gpus,
          gpuModel: params.gpuModel,
          pricePerHour: params.pricePerHour,
          minDurationHours: params.minDurationHours,
          maxDurationHours: params.maxDurationHours,
        });

        return jsonResult({
          success: true,
          offer: {
            id: offer.id,
            gpus: offer.gpus,
            gpuModel: offer.gpuModel,
            pricePerHour: `$${offer.pricePerHour.toFixed(2)}/hr`,
            region: offer.region,
          },
          message: `Offer created: ${offer.gpus}x ${offer.gpuModel} at $${offer.pricePerHour.toFixed(2)}/hr (id: ${offer.id})`,
        });
      } catch (err) {
        return errorResult(`Failed to create MOLT offer: ${err}`);
      }
    },
  };
}

// ─────────────────────────────────────────────────────────────
// molt_bid
// ─────────────────────────────────────────────────────────────

const MoltBidSchema = Type.Object({
  offerId: Type.String({ description: "Offer ID to bid on" }),
  pricePerHour: Type.Number({ description: "Your bid price per hour (USD)" }),
  durationHours: Type.Number({ description: "Rental duration in hours", minimum: 1 }),
});

export function createMoltBidTool(): ClawbernnetesTool {
  return {
    name: "molt_bid",
    label: "MOLT",
    description:
      "Place a bid on a GPU capacity offer. If accepted, the GPUs will be allocated to your workloads.",
    parameters: MoltBidSchema,
    execute: async (_id, args, context) => {
      try {
        const client = getClient(context?.config);
        const params = args as {
          offerId: string;
          pricePerHour: number;
          durationHours: number;
        };

        const bid = await client.placeMoltBid({
          offerId: params.offerId,
          pricePerHour: params.pricePerHour,
          durationHours: params.durationHours,
        });

        const totalCost = bid.pricePerHour * bid.durationHours;

        return jsonResult({
          success: true,
          bid: {
            id: bid.id,
            offerId: bid.offerId,
            pricePerHour: `$${bid.pricePerHour.toFixed(2)}/hr`,
            durationHours: bid.durationHours,
            totalCost: `$${totalCost.toFixed(2)}`,
            status: bid.status,
          },
          message: `Bid placed: $${bid.pricePerHour.toFixed(2)}/hr for ${bid.durationHours}h (total: $${totalCost.toFixed(2)}). Status: ${bid.status}`,
        });
      } catch (err) {
        return errorResult(`Failed to place MOLT bid: ${err}`);
      }
    },
  };
}

// ─────────────────────────────────────────────────────────────
// molt_spot_prices
// ─────────────────────────────────────────────────────────────

const MoltSpotPricesSchema = Type.Object({
  region: Type.Optional(Type.String({ description: "Region filter" })),
  gpuModel: Type.Optional(Type.String({ description: "GPU model filter" })),
});

export function createMoltSpotPricesTool(): ClawbernnetesTool {
  return {
    name: "molt_spot_prices",
    label: "MOLT",
    description: "Get current spot prices for GPU capacity across the MOLT marketplace.",
    parameters: MoltSpotPricesSchema,
    execute: async (_id, args, context) => {
      try {
        const client = getClient(context?.config);
        const params = args as { region?: string; gpuModel?: string };

        const prices = await client.getSpotPrices({
          region: params.region,
          gpuModel: params.gpuModel,
        });

        return jsonResult({
          count: prices.length,
          prices: prices.map((p) => ({
            region: p.region,
            gpuModel: p.gpuModel,
            pricePerHour: `$${p.pricePerHour.toFixed(2)}/hr`,
          })),
          cheapestByModel: Object.entries(
            prices.reduce(
              (acc, p) => {
                if (!acc[p.gpuModel] || p.pricePerHour < acc[p.gpuModel].price) {
                  acc[p.gpuModel] = { price: p.pricePerHour, region: p.region };
                }
                return acc;
              },
              {} as Record<string, { price: number; region: string }>
            )
          ).map(([model, data]) => ({
            gpuModel: model,
            cheapestPrice: `$${data.price.toFixed(2)}/hr`,
            region: data.region,
          })),
        });
      } catch (err) {
        return errorResult(`Failed to get MOLT spot prices: ${err}`);
      }
    },
  };
}

// ─────────────────────────────────────────────────────────────
// Export all MOLT tools
// ─────────────────────────────────────────────────────────────

export function createMoltTools(): ClawbernnetesTool[] {
  return [
    createMoltOffersTool(),
    createMoltOfferCreateTool(),
    createMoltBidTool(),
    createMoltSpotPricesTool(),
  ];
}
