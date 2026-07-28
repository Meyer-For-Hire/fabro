import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import TestRenderer, { act } from "react-test-renderer";

import { setupReactTestEnv } from "../lib/test-utils";
import { SizeChip } from "./size-chip";
import { Tooltip } from "./ui";

let teardownReactTestEnv: (() => void) | undefined;
const mountedRenderers: TestRenderer.ReactTestRenderer[] = [];

function render(element: React.ReactElement): TestRenderer.ReactTestRenderer {
  let renderer: TestRenderer.ReactTestRenderer | undefined;
  act(() => {
    renderer = TestRenderer.create(element);
  });
  mountedRenderers.push(renderer!);
  return renderer!;
}

function tooltipLabel(element: React.ReactElement): string {
  return render(element).root.findByType(Tooltip).props.label as string;
}

describe("SizeChip", () => {
  beforeEach(() => {
    teardownReactTestEnv = setupReactTestEnv();
  });

  afterEach(() => {
    act(() => {
      for (const renderer of mountedRenderers.splice(0)) {
        renderer.unmount();
      }
    });
    teardownReactTestEnv?.();
    teardownReactTestEnv = undefined;
  });

  test("renders the size letter", () => {
    expect(JSON.stringify(render(<SizeChip size="M" />).toJSON())).toContain("M");
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
