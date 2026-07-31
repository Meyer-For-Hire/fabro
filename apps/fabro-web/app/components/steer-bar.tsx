import {
  useImperativeHandle,
  useRef,
  useState,
  type Ref,
} from "react";
import { StopIcon } from "@heroicons/react/20/solid";

import { ApiError } from "../lib/api-client";
import { classNames } from "../lib/class-names";
import { useInterruptRun, useSteerRun } from "../lib/mutations";
import {
  DockComposer,
  RunDockShell,
  DOCK_HEADER_BUTTON,
} from "./run-dock";
import { ErrorMessage } from "./ui";

const STEER_MAX_LENGTH = 8192;

export interface SteerBarProps {
  runId: string;
  waitingForSteer?: boolean;
  ref?: Ref<SteerBarHandle>;
}

export interface SteerBarHandle {
  focus(): void;
}

export function isInterruptDisabled(
  waitingForSteer: boolean,
  mutationPending: boolean,
): boolean {
  return waitingForSteer || mutationPending;
}

/**
 * A run that is waiting for steering needs the operator, so the dock reopens
 * itself and stays open until the wait clears. Derived rather than stored, so
 * collapsing during the wait cannot hide the prompt.
 */
export function isSteerDockCollapsed(
  collapsePreferred: boolean,
  waitingForSteer: boolean,
): boolean {
  return collapsePreferred && !waitingForSteer;
}

export function steerStatusLabel(waitingForSteer: boolean): string {
  return waitingForSteer ? "Interrupted — waiting for steering" : "Steering";
}

export function SteerBar({
  runId,
  waitingForSteer = false,
  ref,
}: SteerBarProps) {
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  // Steering is an occasional control, so the dock starts collapsed and
  // stays out of the way until the operator opens it.
  const [collapsePreferred, setCollapsePreferred] = useState(true);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  const steer = useSteerRun(runId);
  const interrupt = useInterruptRun(runId);
  const pending = steer.isMutating || interrupt.isMutating;
  const interruptDisabled = isInterruptDisabled(waitingForSteer, pending);
  const collapsed = isSteerDockCollapsed(collapsePreferred, waitingForSteer);

  useImperativeHandle(
    ref,
    () => ({
      focus() {
        if (!collapsed) {
          textareaRef.current?.focus();
          return;
        }
        setCollapsePreferred(false);
        setTimeout(() => textareaRef.current?.focus(), 0);
      },
    }),
    [collapsed],
  );

  async function sendSteering(text: string) {
    setErrorMessage(null);
    try {
      await steer.trigger({ text, interrupt: false });
      return true;
    } catch (err) {
      setErrorMessage(formatSteerError(err));
      return false;
    }
  }

  async function fireInterrupt() {
    if (interruptDisabled) return;
    setErrorMessage(null);
    try {
      await interrupt.trigger();
    } catch (err) {
      setErrorMessage(formatInterruptError(err));
    }
  }

  return (
    <RunDockShell
      label="Steer running agent"
      tone={waitingForSteer ? "alert" : "idle"}
      status={steerStatusLabel(waitingForSteer)}
      peek="Send a message to the running agent"
      collapsed={collapsed}
      onCollapsedChange={setCollapsePreferred}
      headerActions={
        // Interrupt acts on the run, not on the message being composed, so it
        // sits with the other run-level controls instead of in the composer.
        <button
          type="button"
          onClick={() => void fireInterrupt()}
          disabled={interruptDisabled}
          className={classNames(
            DOCK_HEADER_BUTTON,
            "text-amber outline-amber/40 hover:bg-amber/15 hover:text-amber focus-visible:outline-amber disabled:hover:bg-overlay disabled:hover:text-amber",
          )}
        >
          <StopIcon className="size-3" aria-hidden="true" />
          {interrupt.isMutating ? "Interrupting…" : "Interrupt"}
        </button>
      }
      actions={
        <>
          <DockComposer
            onSubmit={sendSteering}
            placeholder="Steer the agent…"
            submitLabel="Send"
            pendingLabel="Sending…"
            submitting={steer.isMutating}
            disabled={pending}
            ariaLabel="Steering message"
            maxLength={STEER_MAX_LENGTH}
            textareaRef={textareaRef}
          />
          {errorMessage && <ErrorMessage message={errorMessage} />}
        </>
      }
    />
  );
}

function formatSteerError(err: unknown): string {
  if (err instanceof ApiError) {
    const body = err.body as { code?: string; detail?: string } | null;
    if (body?.code === "use_answer_endpoint") {
      return "Run is blocked on a question; answer the question first.";
    }
    return body?.detail ?? err.message ?? "Steer failed.";
  }
  return "Steer failed; try again.";
}

function formatInterruptError(err: unknown): string {
  if (err instanceof ApiError) {
    const body = err.body as { detail?: string } | null;
    return body?.detail ?? err.message ?? "Interrupt failed.";
  }
  return "Interrupt failed; try again.";
}
