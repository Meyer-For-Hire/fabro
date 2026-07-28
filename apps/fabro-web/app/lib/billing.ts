import type { BilledTokenCounts } from "@qltysh/fabro-api-client";

export interface BillingTokenBucket {
  label: string;
  value: number;
}

export function billableOutputTokens(billing: BilledTokenCounts): number {
  return billing.output_tokens + billing.reasoning_tokens;
}

/** The disjoint token buckets shown in every billing breakdown. */
export function billingTokenBuckets(billing: BilledTokenCounts): BillingTokenBucket[] {
  return [
    { label: "Cache read", value: billing.cache_read_tokens },
    { label: "Cache creation", value: billing.cache_write_tokens },
    { label: "Uncached", value: billing.input_tokens },
    { label: "Output", value: billableOutputTokens(billing) },
  ];
}

export function hasBillingUsage(billing: BilledTokenCounts): boolean {
  return (
    billing.total_tokens !== 0 ||
    (billing.total_usd_micros ?? 0) !== 0 ||
    billingTokenBuckets(billing).some((bucket) => bucket.value !== 0)
  );
}
