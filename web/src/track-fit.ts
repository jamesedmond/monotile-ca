// Constant-velocity object tracking shared by the 2D follow-cam and
// the 3D space-time camera. The instantaneous centroid of a small
// object jitters (phase changes shift its centre of mass), so cameras
// follow a least-squares constant-velocity fit of the centroid track
// over a sliding generation window: flapping averages out of the fit,
// course changes absorb smoothly as the window slides, and behaviour
// is generation-indexed so it is identical at any playback speed.
export class TrackFitter {
  private track: { g: number; x: number; y: number }[] = [];
  private lastGen = -1;

  constructor(private windowSize = 48) {}

  reset(): void {
    this.track.length = 0;
    this.lastGen = -1;
  }

  sample(gen: number, x: number, y: number): void {
    if (gen === this.lastGen) return;
    if (gen < this.lastGen) this.track.length = 0; // looped/reset
    this.lastGen = gen;
    this.track.push({ g: gen, x, y });
    if (this.track.length > this.windowSize) this.track.shift();
  }

  private fit(): { x0: number; y0: number; bx: number; by: number } | null {
    if (this.track.length < 8) return null;
    let sg = 0;
    let sgg = 0;
    let sx = 0;
    let sy = 0;
    let sgx = 0;
    let sgy = 0;
    for (const s of this.track) {
      sg += s.g;
      sgg += s.g * s.g;
      sx += s.x;
      sy += s.y;
      sgx += s.g * s.x;
      sgy += s.g * s.y;
    }
    const n = this.track.length;
    const det = n * sgg - sg * sg;
    if (Math.abs(det) < 1e-9) return null;
    const bx = (n * sgx - sg * sx) / det;
    const by = (n * sgy - sg * sy) / det;
    return { x0: (sx - bx * sg) / n, y0: (sy - by * sg) / n, bx, by };
  }

  /** Fitted position evaluated at generation `gen`, or null if empty. */
  target(gen: number): [number, number] | null {
    const last = this.track[this.track.length - 1];
    if (!last) return null;
    const f = this.fit();
    if (!f) return [last.x, last.y];
    return [f.x0 + f.bx * gen, f.y0 + f.by * gen];
  }

  /** Fitted velocity in world units per generation, if established. */
  velocity(): [number, number] | null {
    const f = this.fit();
    return f ? [f.bx, f.by] : null;
  }
}
