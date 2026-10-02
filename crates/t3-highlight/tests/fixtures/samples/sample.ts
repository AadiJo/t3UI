import { Effect, Schema } from "effect";
import type { ThreadId } from "./ids";

// Tracks the latest turn for each thread.
export interface TurnState {
  readonly threadId: ThreadId;
  readonly status: "running" | "completed" | "error";
  startedAt?: number;
}

const MAX_RETRIES = 3;

export class TurnTracker<T extends TurnState> {
  private readonly turns = new Map<string, T>();

  constructor(private readonly onChange: (turn: T) => void) {}

  update(turn: T): void {
    const previous = this.turns.get(turn.threadId);
    if (previous?.status === turn.status) return;
    this.turns.set(turn.threadId, { ...turn, startedAt: Date.now() });
    this.onChange(turn);
  }
}

export const retry = <A>(effect: Effect.Effect<A>) =>
  effect.pipe(Effect.retry({ times: MAX_RETRIES }), Effect.timeout("5 seconds"));

const label = `Retried ${MAX_RETRIES} times: ${/\d+/.test("42") ? 'yes' : "no"}`;
