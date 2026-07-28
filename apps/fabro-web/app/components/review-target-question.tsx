import { ArrowTopRightOnSquareIcon } from "@heroicons/react/20/solid";
import {
  ReviewTargetKind,
  type ReviewTarget,
} from "@qltysh/fabro-api-client";

const CONTROL_CHARACTER = /[\u0000-\u001f\u007f-\u009f]/u;
const UNSAFE_LINK_DELIMITER = /[<>|]/u;

function unicodeScalarCount(value: string): number {
  return Array.from(value).length;
}

function hasSafeReviewTarget(target: ReviewTarget | null | undefined): target is ReviewTarget {
  if (
    !target ||
    target.kind !== ReviewTargetKind.DOCUMENT ||
    !target.label.trim() ||
    target.label !== target.label.trim() ||
    unicodeScalarCount(target.label) > 200 ||
    CONTROL_CHARACTER.test(target.label) ||
    !target.url ||
    target.url !== target.url.trim() ||
    unicodeScalarCount(target.url) > 2048 ||
    CONTROL_CHARACTER.test(target.url) ||
    UNSAFE_LINK_DELIMITER.test(target.url)
  ) {
    return false;
  }

  try {
    const parsed = new URL(target.url);
    return (
      (parsed.protocol === "http:" || parsed.protocol === "https:") &&
      Boolean(parsed.host) &&
      !parsed.username &&
      !parsed.password
    );
  } catch {
    return false;
  }
}

export function ReviewTargetQuestion({
  reviewTarget,
  fallbackText,
  className,
}: {
  reviewTarget: ReviewTarget | null | undefined;
  fallbackText: string;
  className?: string;
}) {
  if (!hasSafeReviewTarget(reviewTarget)) {
    return <p className={className}>{fallbackText}</p>;
  }

  return (
    <p className={className}>
      Review the{" "}
      <a
        href={reviewTarget.url}
        target="_blank"
        rel="noopener noreferrer"
        referrerPolicy="no-referrer"
        className="inline-flex items-baseline gap-1 font-semibold text-teal-300 underline decoration-teal-500/50 underline-offset-2 transition-colors hover:text-fg focus-visible:rounded-sm focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-teal-500"
      >
        <span>{reviewTarget.label}</span>
        <ArrowTopRightOnSquareIcon
          className="size-3 shrink-0 self-center"
          aria-hidden="true"
        />
      </a>{" "}
      document, then choose the next action.
    </p>
  );
}
