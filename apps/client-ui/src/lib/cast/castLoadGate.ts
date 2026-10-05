/**
 * Which receiver statuses describe the media this client asked for.
 *
 * Between a `loadMedia` request and the receiver acknowledging it, the status
 * still describes the previous media: after a track finishes, every update in
 * that window says "idle / finished" again (the web sender fires one change per
 * property, the Android bridge one per distinct status), and after a skip it
 * reports the old track's position and duration. Taken at face value the first
 * would advance the queue once per update (skipping tracks, or reloading the
 * same one under repeat-one) and the second would count a half listen for a
 * track that has not started.
 */
export class CastLoadGate {
  private seq = 0;
  private awaiting = false;
  private finishHandled = false;

  /** A load is about to be requested; returns its token. */
  begin(): number {
    this.seq += 1;
    this.awaiting = true;
    this.finishHandled = false;
    return this.seq;
  }

  /** False once a newer load (or a reset) superseded `token`. */
  isCurrent(token: number): boolean {
    return token === this.seq;
  }

  /**
   * The receiver accepted (`ok`) or refused the load `token`. A refused load
   * leaves the previous "finished" on the receiver: it must not skip ahead.
   */
  settle(token: number, ok: boolean): void {
    if (token !== this.seq) return;
    this.awaiting = false;
    if (!ok) this.finishHandled = true;
  }

  /** Session ended / restarted: forget the load in flight. */
  reset(): void {
    this.seq += 1;
    this.awaiting = false;
    this.finishHandled = false;
  }

  /** The receiver's status is about our media: it may be mirrored. */
  get settled(): boolean {
    return !this.awaiting;
  }

  /** True once per load, when the receiver reports that media finished. */
  takeFinish(status: { player: string; idleReason: string | null }): boolean {
    if (this.awaiting || this.finishHandled) return false;
    if (status.player !== "idle" || status.idleReason !== "finished") return false;
    this.finishHandled = true;
    return true;
  }
}
