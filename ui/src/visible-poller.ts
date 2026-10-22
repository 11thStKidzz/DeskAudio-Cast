// One completion-based poll, never an accumulating interval. Hidden windows
// keep the existing DOM and ignore stale responses from the previous epoch.
export class VisiblePoller<T> {
  private visible = false;
  private disposed = false;
  private epoch = 0;
  private busy = false;
  private timer: ReturnType<typeof setTimeout> | undefined;

  constructor(private readonly task: () => Promise<T>, private readonly apply: (value: T) => void,
    private readonly intervalMs = 3000, private readonly timeoutMs = 1500) {}

  setVisible(visible: boolean) {
    if (this.disposed || this.visible === visible) return;
    this.visible = visible;
    ++this.epoch;
    if (this.timer !== undefined) clearTimeout(this.timer);
    this.timer = undefined;
    if (visible && !this.busy) void this.tick();
  }

  dispose() {
    this.disposed = true;
    this.visible = false;
    ++this.epoch;
    if (this.timer !== undefined) clearTimeout(this.timer);
    this.timer = undefined;
  }

  private async tick() {
    if (this.busy || this.disposed || !this.visible) return;
    this.busy = true;
    const epoch = this.epoch;
    let timeout: ReturnType<typeof setTimeout> | undefined;
    let settled = false;
    const work = Promise.resolve().then(this.task);
    void work.then(() => { settled = true; }, () => { settled = true; });
    try {
      const value = await Promise.race([
        work,
        new Promise<never>((_, reject) => { timeout = setTimeout(() => reject(new Error('UI poll timeout')), this.timeoutMs); }),
      ]);
      if (!this.disposed && this.visible && epoch === this.epoch) this.apply(value);
    } catch { /* UI-only failure: preserve DOM and let the next bounded poll retry. */ }
    finally {
      if (timeout !== undefined) clearTimeout(timeout);
      // A timeout cannot cancel native IPC. Keep the single-flight guard until
      // that original request settles; never accumulate abandoned requests.
      const finish = () => {
        this.busy = false;
        if (!this.disposed && this.visible) {
          this.timer = setTimeout(() => { this.timer = undefined; void this.tick(); }, this.intervalMs);
        }
      };
      if (settled) finish();
      else void work.then(finish, finish);
    }
  }
}
