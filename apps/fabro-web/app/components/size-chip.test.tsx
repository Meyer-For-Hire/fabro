import { describe, expect, test } from "bun:test";
import TestRenderer, { act } from "react-test-renderer";

import { SizeChip } from "./size-chip";
import { Tooltip } from "./ui";

function tooltipLabel(element: React.ReactElement): string {
  let renderer: TestRenderer.ReactTestRenderer | undefined;
  act(() => {
    renderer = TestRenderer.create(element);
  });
  return renderer!.root.findByType(Tooltip).props.label as string;
}

describe("SizeChip", () => {
  test("renders the size letter", () => {
    let renderer: TestRenderer.ReactTestRenderer | undefined;
    act(() => {
      renderer = TestRenderer.create(<SizeChip size="M" />);
    });

    expect(JSON.stringify(renderer!.toJSON())).toContain("M");
  });

  test("appends the cost to the tooltip", () => {
    expect(tooltipLabel(<SizeChip size="M" totalUsdMicros={12_340_000} />))
      .toBe("Size M · $12.34");
  });

  test("omits the cost when the run has no billing yet", () => {
    expect(tooltipLabel(<SizeChip size="M" />)).toBe("Size M");
    expect(tooltipLabel(<SizeChip size="M" totalUsdMicros={null} />)).toBe("Size M");
  });

  test("calls out the tiers that warrant attention", () => {
    expect(tooltipLabel(<SizeChip size="L" totalUsdMicros={150_000_000} />))
      .toBe("Size L (risky) · $150.00");
    expect(tooltipLabel(<SizeChip size="XL" />)).toBe("Size XL (unhealthy)");
  });
});
