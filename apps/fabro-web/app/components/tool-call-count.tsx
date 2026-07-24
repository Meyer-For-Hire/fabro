import { WrenchScrewdriverIcon } from "@heroicons/react/16/solid";

import { plural } from "../lib/plural";

export function ToolCallCount({
  count,
  errored = 0,
  className = "",
}: {
  count: number;
  errored?: number;
  className?: string;
}) {
  return (
    <div
      className={`inline-flex items-center gap-1.5 text-xs text-fg-muted ${className}`}
    >
      <WrenchScrewdriverIcon className="size-3.5" aria-hidden="true" />
      <span>
        {count} tool {plural(count, "call", "calls")}
        {errored > 0 && (
          <span className="text-coral">
            , {errored} with {plural(errored, "an error", "errors")}
          </span>
        )}
      </span>
    </div>
  );
}
