// Coalesce rapid dragging: one request in flight and only the latest pending value.
export class LatestVolume {
  private pending: number | undefined;
  private running = false;
  constructor(private readonly send: (volume: number) => Promise<unknown>, private readonly onError: (error: unknown) => void) {}
  set(volume: number) {
    this.pending = volume;
    if (!this.running) void this.flush();
  }
  private async flush() {
    this.running = true;
    try {
      while (this.pending !== undefined) {
        const volume = this.pending;
        this.pending = undefined;
        try { await this.send(volume); } catch (error) { this.onError(error); }
      }
    } finally { this.running = false; }
  }
}
