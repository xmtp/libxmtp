export class Timestamp {
  constructor(readonly ns: bigint) {}

  get date(): Date {
    const milliseconds = this.ns / 1_000_000n;
    const remainder = this.ns % 1_000_000n;
    return new Date(Number(milliseconds - (remainder < 0n ? 1n : 0n)));
  }
}
